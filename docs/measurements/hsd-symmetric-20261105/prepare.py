"""Prepare fixed dictionaries and public corpora for the symmetric-entry experiment.

Usage: python docs/measurements/hsd-symmetric-20261105/prepare.py TRIAL
Download ldcc-20140209.tar.gz and literature.zip into TRIAL first.
"""

import hashlib
import importlib.util
import json
import re
import struct
import subprocess
import sys
import zipfile
from pathlib import Path


def fingerprint(path):
    """Describe file content.

    Returns:
        Its byte length and SHA-256.

    """
    return {
        "bytes": path.stat().st_size,
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
    }


def prepare(trial):
    """Write isolated dictionaries and normalized corpus files."""
    repo = Path.cwd()
    spec = importlib.util.spec_from_file_location(
        "storage", repo / "scripts/hsd-numeric-analyze.py"
    )
    storage = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(storage)
    prior = json.loads(
        (repo / "docs/measurements/hsd-numeric-20261105/summary.json").read_text(
            encoding="utf-8"
        )
    )
    records = {row["dictionary"]: row for row in prior["dictionaries"]}
    manifest = {
        "baseline_head": "e7a65059272e07087d555575b13b2ed53d1d3e28",
        "dictionaries": {},
        "corpora": {},
    }
    for mode in ["v5", "symmetric"]:
        (trial / mode).mkdir(exist_ok=True)
    for name, expected in records.items():
        source = repo / "dict" / f"{name}.hsd"
        assert fingerprint(source)["sha256"] == expected["sha256"]
        data = source.read_bytes()
        sections = storage.read_sections(data)
        packed = bytearray()
        restored = bytearray()
        for left_last, right, cost in struct.iter_unpack("<HHh", sections[6]):
            assert left_last & 0x7FFF == right
            packed += struct.pack("<Hh", left_last, cost)
            restored += struct.pack("<HHh", left_last, left_last & 0x7FFF, cost)
        assert restored == sections[6]
        header = bytearray(data)
        struct.pack_into("<I", header, 12, struct.unpack_from("<I", header, 12)[0] | 2)
        changed = dict(sections)
        changed[6] = packed
        destination = trial / "symmetric" / source.name
        storage.write_container(header, changed, destination, 5013)
        (trial / "v5" / source.name).write_bytes(data)
        manifest["dictionaries"][name] = {
            "v5": fingerprint(source),
            "symmetric": fingerprint(destination),
        }
    corpus = trial / "corpus"
    subprocess.run(
        [
            sys.executable,
            str(repo / "scripts/hsd-format-corpus.py"),
            str(trial / "ldcc-20140209.tar.gz"),
            str(trial / "v5-source"),
            str(corpus),
        ],
        check=True,
    )
    with zipfile.ZipFile(trial / "literature.zip") as archive:
        names = [n for n in archive.namelist() if n.lower().endswith(".txt")]
        assert len(names) == 1
        raw = archive.read(names[0]).decode("shift_jis")
    parts = raw.split("-------------------------------------------------------")
    assert len(parts) >= 3
    body = parts[2].split("底本：")[0]
    body = re.sub(r"《[^》]*》|［＃[^］]*］|｜", "", body)
    lines = [line.strip() for line in body.splitlines() if line.strip()]
    literary = "\n".join(lines) + "\n"
    (corpus / "literature.txt").write_text(
        literary * 32, encoding="utf-8", newline="\n"
    )
    (corpus / "short.txt").write_text(
        "東京都に住んでいる人々が増えている。\n" * 100_000,
        encoding="utf-8",
        newline="\n",
    )
    quantities = []
    for number in [
        "1",
        "4",
        "7",
        "9",
        "11",
        "12",
        "2024",
        "２０２４",
        "一一",
        "-3.5",
        "1,234",
        "０．５",
    ]:
        for suffix in [
            "月",
            "月1日",
            "年3月1日",
            "日間",
            "kg",
            "km/h",
            "m/s",
            "cm²",
            "人",
            "段",
        ]:
            quantities.extend(
                [
                    f"測定値は{number}{suffix}です。",
                    f"名前はID_{number}{suffix}_Aです。",
                    f"「{number}{suffix}」と記録した。",
                ]
            )
    (corpus / "quantities.txt").write_text(
        "\n".join(quantities) + "\n", encoding="utf-8", newline="\n"
    )
    with (corpus / "mixed.txt").open("a", encoding="utf-8", newline="\n") as stream:
        stream.write("\n".join(quantities) + "\n")
    for name in ["mixed", "short", "literature", "quantities"]:
        path = corpus / f"{name}.txt"
        manifest["corpora"][name] = {
            **fingerprint(path),
            "lines": len(path.read_text(encoding="utf-8").splitlines()),
        }
    manifest["literature"] = {
        "source_url": "https://www.aozora.gr.jp/cards/000148/files/752_ruby_2438.zip",
        "archive": fingerprint(trial / "literature.zip"),
        "source_lines": len(lines),
        "repetitions": 32,
    }
    manifest["short"] = {"repetitions": 100_000}
    manifest["news_archive"] = fingerprint(trial / "ldcc-20140209.tar.gz")
    (trial / "input-manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    print("Prepared three dictionaries and four fixed corpora.")


if __name__ == "__main__":
    prepare(Path(sys.argv[1]).resolve())
