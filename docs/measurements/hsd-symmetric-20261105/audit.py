"""Audit published counts, source archives, equality records and performance.

Usage: python docs/measurements/hsd-symmetric-20261105/audit.py
No dictionaries, binaries, downloads or private inputs are needed.
"""

import hashlib
import json
import statistics
import zipfile
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def read_json(relative):
    """Read one structured record."""
    return json.loads((ROOT / relative).read_text(encoding="utf-8"))


def rows(relative):
    """Read a JSONL series."""
    return [json.loads(line) for line in (ROOT / relative).read_text().splitlines()]


def identity(data):
    """Return a byte count and SHA-256."""
    return {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def main():
    """Check raw data and print independently recomputed summary values."""
    manifest = read_json("results/manifest.json")
    with zipfile.ZipFile(ROOT / "results/measured-sources.zip") as archive:
        for group in ["runtime_sources", "measurement_sources"]:
            for name, expected in manifest[group].items():
                assert identity(archive.read(name)) == expected, name
    analysis = rows("results/analysis.jsonl")
    assert len(analysis) == 216
    assert len(rows("results/load.jsonl")) == 2232
    assert len(rows("results/load-once.jsonl")) == 72
    counts = Counter((r["corpus"], r["dictionary"], r["variant"]) for r in analysis)
    assert len(counts) == 18 and set(counts.values()) == {12}
    summary = read_json("results/summary.json")
    for corpus in ["mixed", "short", "literature"]:
        for name in manifest["dictionaries"]:
            subset = [
                r for r in analysis if r["corpus"] == corpus and r["dictionary"] == name
            ]
            pairs = {
                i: {r["variant"]: r for r in subset if r["round"] == i}
                for i in range(12)
            }
            changes = [p["patch"]["wall"] / p["v5"]["wall"] - 1 for p in pairs.values()]
            record = summary["analysis"][corpus][name]
            assert (
                statistics.median(changes)
                == record["paired_wall_relative_change"]["median"]
            )
            medians = {
                v: statistics.median(r["wall"] for r in subset if r["variant"] == v)
                for v in ["v5", "patch"]
            }
            assert (
                medians["patch"] / medians["v5"] - 1
                == record["wall_median_relative_change"]
            )
            print(corpus, name, medians, "paired change", statistics.median(changes))
    confirmation = rows("confirmation/analysis.jsonl")
    assert len(confirmation) == 24 and all(len(r["passes"]) == 8 for r in confirmation)
    paired = []
    for i in range(12):
        pair = {r["variant"]: r for r in confirmation if r["round"] == i}
        paired.append(pair["patch"]["wall"] / pair["v5"]["wall"] - 1)
        for r in pair.values():
            assert r["wall"] == statistics.mean(p["wall"] for p in r["passes"])
    confirm_summary = read_json("confirmation/summary.json")
    assert statistics.median(paired) == confirm_summary["paired_median"]
    assert all(value < 0 for value in paired)
    assert confirm_summary["executables"] == manifest["executables"]
    with zipfile.ZipFile(ROOT / "confirmation/sources.zip") as archive:
        assert identity(archive.read("confirm.py")) == confirm_summary["source"]
    rebuilt = read_json("rebuilt-validation.json")
    compressed = rows("rebuilt-compression.jsonl")
    assert len(compressed) == 6
    for name, dictionary in rebuilt["dictionaries"].items():
        assert (
            dictionary["all_other_sections_equal"]
            and dictionary["entries_restored_equal"]
        )
        assert len(rebuilt["all_fields"][name]) == 4
        assert (
            read_json("python-validation.json")[name]
            == rebuilt["all_fields"][name]["quantities"]
        )
        for variant in ["v5", "v6"]:
            row = next(
                r
                for r in compressed
                if r["dictionary"] == name and r["variant"] == variant
            )
            assert (
                row["source_sha256"]
                == row["decompressed_sha256"]
                == dictionary[variant]["sha256"]
            )
            assert row["file_bytes"] == dictionary[variant]["bytes"]
    assert rebuilt["sentence_exceptions_equal"]
    print("confirmation paired change", statistics.median(paired))
    print("All published counts, identities and equality records agree.")


if __name__ == "__main__":
    main()
