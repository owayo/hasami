"""Compress both complete dictionaries with the same official zstd binary.

Usage: mise exec -- python docs/measurements/hsd-tail-pool-20261022/compress.py TRIAL
Run before or after the analysis measurements, never concurrently with them.
"""

import hashlib
import json
import subprocess
import sys
import time
from pathlib import Path

trial = Path(sys.argv[1]).resolve()
repo = Path(__file__).resolve().parents[3]
zstd = trial / "zstd/zstd-v1.5.7-win64/zstd.exe"
output = trial / "compressed"
output.mkdir(exist_ok=True)


def fingerprint(path):
    """Identify a file by bytes and SHA-256.

    Returns:
        The byte count and SHA-256 of the file.

    """
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"bytes": path.stat().st_size, "sha256": digest}


with (trial / "compression.jsonl").open("x", encoding="utf-8", newline="\n") as log:
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"]:
        for variant in ["v5", "pool"]:
            folder = (
                repo / "target/hsd-repeat-windows/v5"
                if variant == "v5"
                else trial / "pool"
            )
            source = folder / f"{name}.hsd"
            compressed = output / f"{name}-{variant}.hsd.zst"
            command = [
                str(zstd),
                "-q",
                "-19",
                "-T1",
                str(source),
                "-o",
                str(compressed),
            ]
            start = time.perf_counter()
            subprocess.run(command, check=True)
            wall = time.perf_counter() - start
            with subprocess.Popen(
                [str(zstd), "-q", "-d", "-c", str(compressed)], stdout=subprocess.PIPE
            ) as process:
                restored_hash = hashlib.file_digest(
                    process.stdout, "sha256"
                ).hexdigest()
                assert process.wait() == 0
            identity = fingerprint(source)
            assert restored_hash == identity["sha256"]
            row = {
                "dictionary": name,
                "variant": variant,
                "input": identity,
                "compressed": fingerprint(compressed),
                "wall_seconds": wall,
                "zstd_version": subprocess.check_output(
                    [str(zstd), "--version"], text=True
                ).strip(),
                "zstd_executable": fingerprint(zstd),
                "zstd_release_zip": fingerprint(trial / "zstd-v1.5.7-win64.zip"),
                "flags": ["-19", "-T1"],
                "decompressed_sha256_equal": True,
            }
            log.write(json.dumps(row) + "\n")
            log.flush()
            print(name, variant, row["compressed"]["bytes"], flush=True)
