"""Compare complete upstream-rebuilt v5 and v6 dictionaries with zstd 1.5.7.

Usage: python docs/measurements/hsd-symmetric-20261105/compress.py TRIAL ZSTD
"""

import hashlib
import importlib.util
import json
import subprocess
import sys
from pathlib import Path


def main(trial, executable):
    """Compress and round-trip each complete dictionary with one worker."""
    spec = importlib.util.spec_from_file_location(
        "codec", Path("scripts/hsd-numeric-compress.py")
    )
    codec = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(codec)
    version = subprocess.check_output(
        [str(executable), "--version"], encoding="utf-8"
    ).strip()
    with (trial / "rebuilt-compression.jsonl").open(
        "w", encoding="utf-8", newline="\n"
    ) as log:
        for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"]:
            for mode in ["v5", "v6"]:
                source = trial / f"rebuild-{mode}" / f"{name}.hsd"
                target = source.with_suffix(".hsd.zst")
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
                digest = hashlib.sha256(source.read_bytes()).hexdigest()
                restored = codec.verify_compressed(executable, target, digest)
                row = {
                    "dictionary": name,
                    "variant": mode,
                    "file_bytes": source.stat().st_size,
                    "source_sha256": digest,
                    "compressed_bytes": target.stat().st_size,
                    "compressed_sha256": hashlib.sha256(
                        target.read_bytes()
                    ).hexdigest(),
                    "decompressed_sha256": restored,
                    "zstd": version,
                    "options": ["-19", "-T1"],
                }
                log.write(json.dumps(row) + "\n")
                log.flush()
                print(f"compressed and restored: {name} {mode}", flush=True)


if __name__ == "__main__":
    main(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve())
