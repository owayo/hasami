"""Collect public aggregate measurements and freeze their reproducible source bytes.

Usage: python docs/measurements/hsd-numeric-20261105/collect.py TRIAL VERIFIED_TRIAL
Run from the repository root after all fifteen compression results are available.
"""

import hashlib
import json
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path


def collect(trial, verified):
    """Save aggregate evidence, omitting private input-repository metadata."""
    output = Path(__file__).parent
    names = ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"]
    dictionaries = []
    for name in names:
        record = json.loads((trial / f"{name}.json").read_text(encoding="utf-8"))
        metadata = dict(
            line.split("=", 1) for line in record.pop("metadata").splitlines()
        )
        record["metadata"] = {
            key: metadata[key] for key in ["name", "hasami_version", "pos_scheme"]
        }
        record["numeric_features"] = json.loads(
            (trial / f"{name}-features.json").read_text(encoding="utf-8")
        )
        second = json.loads((verified / f"{name}.json").read_text(encoding="utf-8"))
        assert (
            record["symmetric_prototype"]["sha256"]
            == second["symmetric_prototype"]["sha256"]
        )
        assert record["pair_prototype"]["sha256"] == second["pair_prototype"]["sha256"]
        for left, right in zip(record["numeric"], second["numeric"], strict=True):
            assert left["factor_prototype"].get("sha256") == right[
                "factor_prototype"
            ].get("sha256")
        second_features = json.loads(
            (verified / f"{name}-features.json").read_text(encoding="utf-8")
        )
        assert record["numeric_features"]["sha256"] == second_features["sha256"]
        record["independent_generation_hashes_equal"] = True
        dictionaries.append(record)
        (output / f"verify-{name}.txt").write_text(
            (trial / f"verify-{name}.txt").read_text(encoding="utf-8"),
            encoding="utf-8",
            newline="\n",
        )
    compression = [
        json.loads(line)
        for line in (trial / "compression.jsonl")
        .read_text(encoding="utf-8")
        .splitlines()
    ]
    assert len(compression) == 15
    assert len({(r["dictionary"], r["variant"]) for r in compression}) == 15
    for row in compression:
        name, label = row["dictionary"], row["variant"]
        source = (
            Path("dict") / f"{name}.hsd"
            if label == "v5"
            else trial / f"{name}-{label}.hsd"
        )
        assert row["source_sha256"] == hashlib.sha256(source.read_bytes()).hexdigest()
        assert row["decompressed_sha256"] == row["source_sha256"]
        compressed = trial / f"{name}-{label}.hsd.zst"
        assert (
            row["compressed_sha256"]
            == hashlib.sha256(compressed.read_bytes()).hexdigest()
        )
        assert row["compressed_bytes"] == compressed.stat().st_size
    paths = [
        Path("scripts") / name
        for name in [
            "hsd-numeric-probe.rs",
            "hsd-numeric-analyze.py",
            "hsd-numeric-features.py",
            "hsd-numeric-compress.py",
        ]
    ]
    paths.append(Path(__file__))
    paths.append(Path("scripts/hsd-format-eval.rs"))
    with zipfile.ZipFile(
        output / "measured-sources.zip", "w", zipfile.ZIP_DEFLATED
    ) as archive:
        for path in paths:
            archive.writestr(path.as_posix(), path.read_bytes())
    summary = {
        "date": "2026-11-05",
        "baseline_head": "3b95cde736cc618d2e904b5f79deb138434b69c8",
        "comparison_start": "78f63181f2f436f8dd819fee5e2ff9d422fa1b52",
        "format": 5,
        "cpu": "AMD Ryzen 7 5700X",
        "os": "Microsoft Windows 10.0.26300",
        "rustc": "rustc 1.98.1 (48a229cea 2026-09-01)",
        "python": "3.13.15",
        "source_sha256": {
            path.as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in paths
        },
        "source_zip_sha256": hashlib.sha256(
            (output / "measured-sources.zip").read_bytes()
        ).hexdigest(),
        "dictionaries": dictionaries,
        "compression": compression,
        "analyzer_variant_benchmark": "not implemented or measured",
        "memory_comparison": "not measured",
        "production_format_changed": False,
    }
    (output / "summary.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    shutil.copyfile(trial / "compression.jsonl", output / "compression.jsonl")
    log = subprocess.check_output(
        [
            "git",
            "log",
            "--format=%h %s",
            f"{summary['comparison_start']}..{summary['baseline_head']}",
        ],
        encoding="utf-8",
    )
    (output / "commits.txt").write_text(log, encoding="utf-8", newline="\n")
    print(
        "Collected three verified dictionary studies and fifteen compression records."
    )


if __name__ == "__main__":
    collect(Path(sys.argv[1]), Path(sys.argv[2]))
