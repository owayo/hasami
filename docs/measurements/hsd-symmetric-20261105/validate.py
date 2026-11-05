"""Validate the v5/v6 rebuilds, all token fields, and the downstream rule pipeline.

Usage: python docs/measurements/hsd-symmetric-20261105/validate.py TRIAL
Requires eval-v6.exe, both rebuilt dictionary sets, and both downstream examples.
"""

import hashlib
import importlib.util
import json
import struct
import subprocess
import sys
from pathlib import Path

NAMES = ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"]


def fingerprint(path):
    """Identify a file.

    Returns:
        Byte count and SHA-256.

    """
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"bytes": path.stat().st_size, "sha256": digest}


def compare_commands(commands):
    """Compare complete output streams directly.

    Returns:
        Equal byte count and SHA-256.

    """
    processes = [
        subprocess.Popen(command, stdout=subprocess.PIPE) for command in commands
    ]
    digest = hashlib.sha256()
    count = 0
    try:
        while True:
            values = [process.stdout.read(65536) for process in processes]
            assert values[0] == values[1], count
            if not values[0]:
                break
            digest.update(values[0])
            count += len(values[0])
        assert all(process.wait() == 0 for process in processes)
    finally:
        for process in processes:
            if process.poll() is None:
                process.terminate()
                process.wait()
            process.stdout.close()
    return {"equal_bytes": count, "sha256": digest.hexdigest()}


def main(trial):
    """Check rebuilt artifacts independently of their physical layout."""
    repo = Path.cwd()
    spec = importlib.util.spec_from_file_location(
        "storage", repo / "scripts/hsd-numeric-analyze.py"
    )
    storage = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(storage)
    result = {"dictionaries": {}, "all_fields": {}, "kotonoha": {}}
    for name in NAMES:
        old_path, new_path = [
            trial / f"rebuild-{v}" / f"{name}.hsd" for v in ["v5", "v6"]
        ]
        old, new = [path.read_bytes() for path in [old_path, new_path]]
        a, b = storage.read_sections(old), storage.read_sections(new, 6)
        assert a.keys() == b.keys()
        assert all(a[sid] == b[sid] for sid in a if sid != 6)
        assert struct.unpack_from("<I", new, 12)[0] & 2
        restored = bytearray()
        for context, cost in struct.iter_unpack("<Hh", b[6]):
            restored += struct.pack("<HHh", context, context & 0x7FFF, cost)
        assert restored == a[6]
        result["dictionaries"][name] = {
            "v5": fingerprint(old_path),
            "v6": fingerprint(new_path),
            "entries": len(b[6]) // 4,
            "all_other_sections_equal": True,
            "entries_restored_equal": True,
        }
        for mode in ["v5", "v6"]:
            with (trial / f"verify-rebuild-{mode}-{name}.txt").open(
                "w", encoding="utf-8"
            ) as log:
                subprocess.run(
                    [
                        str(trial / f"eval-{mode}.exe"),
                        "verify",
                        str(trial / f"rebuild-{mode}" / f"{name}.hsd"),
                    ],
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    check=True,
                )
        result["all_fields"][name] = {}
        for corpus in ["mixed", "short", "literature", "quantities"]:
            commands = [
                [
                    str(trial / f"eval-{mode}.exe"),
                    "dump",
                    str(trial / f"rebuild-{mode}" / f"{name}.hsd"),
                    str(trial / "corpus" / f"{corpus}.txt"),
                ]
                for mode in ["v5", "v6"]
            ]
            result["all_fields"][name][corpus] = compare_commands(commands)
        commands = [
            [
                str(
                    trial
                    / f"kotonoha-{mode}/target/release/examples/hsd_format_check.exe"
                ),
                str(trial / f"rebuild-{mode}" / f"{name}.hsd"),
                str(trial / "corpus/kotonoha-check.txt"),
            ]
            for mode in ["v5", "v6"]
        ]
        result["kotonoha"][name] = compare_commands(commands)
        print(
            f"Rebuilt dictionaries, all fields and downstream pipeline equal: {name}",
            flush=True,
        )
    exception_outputs = []
    for mode, executable in [
        ("v5", trial / "v5-source/target/release/hasami.exe"),
        ("v6", repo / "target/release/hasami.exe"),
    ]:
        output = trial / f"exceptions-{mode}.txt"
        subprocess.run(
            [
                str(executable),
                "export-sentence-exceptions",
                "--dict",
                str(trial / f"rebuild-{mode}/ipadic-neologd-sudachi.hsd"),
                "--output",
                str(output),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        dictionary_path = str(
            trial / f"rebuild-{mode}/ipadic-neologd-sudachi.hsd"
        ).encode("utf-8")
        canonical_path = b"dict/ipadic-neologd-sudachi.hsd"
        exception_outputs.append(
            b"".join(
                line.replace(dictionary_path, canonical_path)
                if line.startswith(b"#")
                else line
                for line in output.read_bytes().splitlines(keepends=True)
            )
        )
    assert (
        exception_outputs[0]
        == exception_outputs[1]
        == (repo / "src/sentence/builtin_exceptions.txt").read_bytes()
    )
    result["sentence_exceptions_equal"] = True
    result["sentence_exception_source_path_normalized"] = True
    (trial / "rebuilt-validation.json").write_text(
        json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n"
    )


if __name__ == "__main__":
    main(Path(sys.argv[1]).resolve())
