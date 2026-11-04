"""Compare complete prototype files using one zstd worker, without replacing inputs.

Usage: python scripts/hsd-numeric-compress.py TRIAL ZSTD DICT...
The stored times describe compression, not dictionary loading or tokenization.
"""

import hashlib
import json
import subprocess
import sys
import time
from pathlib import Path


def verify_compressed(executable, target, expected):
    """Stream-decompress a result without writing a second dictionary.

    Returns:
        The SHA-256 of the restored dictionary bytes.

    """
    digest = hashlib.sha256()
    process = subprocess.Popen(
        [str(executable), "-q", "-d", "-c", str(target)], stdout=subprocess.PIPE
    )
    with process.stdout:
        for block in iter(lambda: process.stdout.read(1024 * 1024), b""):
            digest.update(block)
    assert process.wait() == 0
    actual = digest.hexdigest()
    assert actual == expected
    return actual


def compress(trial, executable, dictionaries):
    """Write one reproducible JSONL record per complete file."""
    version = subprocess.check_output([str(executable), "--version"], text=True).strip()
    log = trial / "compression.jsonl"
    cached = {}
    if log.exists():
        for line in log.read_text(encoding="utf-8").splitlines():
            record = json.loads(line)
            cached[
                (record["dictionary"], record["variant"], record["source_sha256"])
            ] = record
    with log.open("w", encoding="utf-8", newline="\n") as out:
        for dictionary in dictionaries:
            name = dictionary.stem
            variants = [("v5", dictionary)] + [
                (label, trial / f"{name}-{label}.hsd")
                for label in [
                    "symmetric",
                    "features",
                    "quantity_suffix-factor",
                    "month-factor",
                ]
            ]
            for label, source in variants:
                target = trial / f"{name}-{label}.hsd.zst"
                digest = hashlib.sha256(source.read_bytes()).hexdigest()
                previous = cached.get((name, label, digest))
                if (
                    previous
                    and previous["zstd"] == version
                    and target.exists()
                    and hashlib.sha256(target.read_bytes()).hexdigest()
                    == previous["compressed_sha256"]
                ):
                    previous["decompressed_sha256"] = verify_compressed(
                        executable, target, digest
                    )
                    out.write(json.dumps(previous) + "\n")
                    out.flush()
                    print(f"reused {name} {label}", flush=True)
                    continue
                start = time.perf_counter()
                subprocess.run(
                    [
                        str(executable),
                        "-q",
                        "-19",
                        "-T1",
                        "-f",
                        str(source),
                        "-o",
                        str(target),
                    ],
                    check=True,
                )
                result = {
                    "dictionary": name,
                    "variant": label,
                    "zstd": version,
                    "file_bytes": source.stat().st_size,
                    "source_sha256": digest,
                    "compressed_bytes": target.stat().st_size,
                    "compressed_sha256": hashlib.sha256(
                        target.read_bytes()
                    ).hexdigest(),
                    "compress_seconds": time.perf_counter() - start,
                }
                result["decompressed_sha256"] = verify_compressed(
                    executable, target, digest
                )
                line = json.dumps(result)
                out.write(line + "\n")
                out.flush()
                print(line, flush=True)


if __name__ == "__main__":
    compress(Path(sys.argv[1]), Path(sys.argv[2]), [Path(p) for p in sys.argv[3:]])
