"""Collect public-safe results from an isolated trial, without corpus or binaries.

Usage: python docs/measurements/hsd-symmetric-20261105/collect.py TRIAL
Run from the repository root after all validation commands succeed.
"""

import hashlib
import json
import re
import shutil
import sys
from pathlib import Path


def fingerprint(path):
    """Return the file size and digest."""
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"bytes": path.stat().st_size, "sha256": digest}


def main(trial):
    """Copy the explicit allowlist and summarize test results."""
    output = Path(__file__).resolve().parent
    for directory, files in {
        "results": [
            "analysis.jsonl",
            "load.jsonl",
            "load-once.jsonl",
            "manifest.json",
            "summary.json",
            "measured-sources.zip",
        ],
        "confirmation": ["analysis.jsonl", "summary.json", "sources.zip"],
    }.items():
        destination = output / directory
        destination.mkdir(exist_ok=True)
        for name in files:
            shutil.copyfile(trial / directory / name, destination / name)
    for name in [
        "rebuilt-validation.json",
        "rebuilt-compression.jsonl",
        "python-validation.json",
    ]:
        shutil.copyfile(trial / name, output / name)
    checks = {}
    for name in ["make-ci.txt", "distributed-tests.txt", "kotonoha-v6-ci.txt"]:
        path = trial / name
        raw = path.read_bytes()
        # PowerShell redirection can produce UTF-16; preserve only summary lines.
        content = raw.decode("utf-16" if raw.startswith(b"\xff\xfe") else "utf-8-sig")
        results = re.findall(
            r"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;"
            r" (\d+) measured; (\d+) filtered out;",
            content,
        )
        assert results and all(row[0] == "ok" and int(row[2]) == 0 for row in results)
        checks[name] = {
            "log": fingerprint(path),
            "test_results": [
                dict(
                    zip(
                        ["passed", "failed", "ignored", "measured", "filtered_out"],
                        map(int, row[1:]),
                    )
                )
                for row in results
            ],
            "note": "Each feature configuration is a separate test run; counts overlap.",
        }
    (output / "checks.json").write_text(
        json.dumps(checks, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    sources = {}
    for path in sorted(Path("src").rglob("*.rs")):
        sources[path.as_posix()] = fingerprint(path)
    (output / "final-sources.json").write_text(
        json.dumps(sources, indent=2) + "\n", encoding="utf-8", newline="\n"
    )


if __name__ == "__main__":
    main(Path(sys.argv[1]).resolve())
