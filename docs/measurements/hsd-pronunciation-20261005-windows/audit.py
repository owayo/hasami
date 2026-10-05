"""Recompute the saved statistics and check the complete Windows run order."""

import hashlib
import json
import math
import re
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
NAMES = ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"]
CORPORA = ["mixed", "short", "literature"]
TOKENS = {
    "mixed": [5183475, 4859437, 4852226],
    "short": [1200000, 1100000, 1100000],
    "literature": [5711900, 5622300, 5579900],
}


def percentile(values, fraction):
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def check_stats(actual, values):
    mean = sum(values) / len(values)
    expected = {
        "n": len(values),
        "median": percentile(values, 0.5),
        "min": min(values),
        "max": max(values),
        "iqr": percentile(values, 0.75) - percentile(values, 0.25),
        "cv": math.sqrt(sum((value - mean) ** 2 for value in values) / len(values))
        / mean
        if mean
        else None,
    }
    for key, value in expected.items():
        assert (
            actual[key] is None
            if value is None
            else math.isclose(actual[key], value, rel_tol=1e-9, abs_tol=1e-9)
        ), (key, actual[key], value)


def read_rows(name):
    return [json.loads(line) for line in (ROOT / name).read_text().splitlines()]


analysis = read_rows("analysis.jsonl")
loads = read_rows("load.jsonl")
summary = json.loads((ROOT / "summary.json").read_text())
manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
with zipfile.ZipFile(ROOT / "measured-sources.zip") as archive:
    for name, expected_identity in manifest["scripts"].items():
        data = archive.read(name)
        assert len(data) == expected_identity["bytes"]
        assert hashlib.sha256(data).hexdigest() == expected_identity["sha256"]
expected = []
expected_loads = []
for corpus in CORPORA:
    for turn in range(12):
        names = NAMES[turn % 3 :] + NAMES[: turn % 3]
        for name in names:
            order = ["v5", "patch"] if turn % 2 == 0 else ["patch", "v5"]
            for position, variant in enumerate(order):
                expected.append((corpus, turn, name, position, variant))
                if corpus == "mixed":
                    for sample in range(31):
                        expected_loads.append((turn, name, position, variant, sample))
assert len(analysis) == 216 and len(loads) == 2232
assert [
    tuple(
        row[key]
        for key in ["corpus", "round", "dictionary", "position_in_pair", "variant"]
    )
    for row in analysis
] == expected
assert [
    tuple(
        row[key]
        for key in ["round", "dictionary", "position_in_pair", "variant", "sample"]
    )
    for row in loads
] == expected_loads
for row in analysis:
    assert row["tokens"] == TOKENS[row["corpus"]][NAMES.index(row["dictionary"])]
    assert row["wall"] > 0 and row["cpu"] > 0 and row["rss_bytes"] > 0

orders = {}
for corpus in CORPORA:
    orders[corpus] = {}
    for name in NAMES:
        rows = [
            r for r in analysis if r["corpus"] == corpus and r["dictionary"] == name
        ]
        saved = summary["analysis"][corpus][name]
        for variant in ["v5", "patch"]:
            group = [r for r in rows if r["variant"] == variant]
            for field in ["wall", "cpu", "rss_bytes"]:
                check_stats(saved[variant][field], [r[field] for r in group])
        orders[corpus][name] = {}
        for field in ["wall", "cpu"]:
            ratios = []
            for turn in range(12):
                pair = {r["variant"]: r for r in rows if r["round"] == turn}
                ratios.append(pair["patch"][field] / pair["v5"][field] - 1)
            check_stats(saved[f"paired_{field}_relative_change"], ratios)
            assert saved[f"paired_{field}_relative_changes"] == ratios
            assert saved[
                "pairs_patch_wall_slower" if field == "wall" else "pairs_patch_slower"
            ] == sum(ratio > 0 for ratio in ratios)
            assert math.isclose(
                saved[f"{field}_median_relative_change"],
                saved["patch"][field]["median"] / saved["v5"][field]["median"] - 1,
            )
            orders[corpus][name][field] = {
                "AB_median": percentile(ratios[::2], 0.5),
                "BA_median": percentile(ratios[1::2], 0.5),
            }
for name in NAMES:
    for variant in ["v5", "patch"]:
        rows = [r for r in loads if r["dictionary"] == name and r["variant"] == variant]
        for scope, first in [("first_in_process", True), ("reconstructed", False)]:
            for field in ["wall_us", "cpu_us"]:
                check_stats(
                    summary["load"][name][variant][scope][field],
                    [r[field] for r in rows if (r["sample"] == 0) == first],
                )

utilization = [row["system_cpu_percent_during_process"] for row in analysis]
comparisons = {}
for corpus in CORPORA:
    comparisons[corpus] = {}
    for name in NAMES:
        text = (ROOT / f"compare-{corpus}-{name}.txt").read_text()
        match = re.search(r"equal bytes=(\d+) sha256=([0-9a-f]{64})", text)
        assert match, (corpus, name)
        comparisons[corpus][name] = {
            "equal_bytes": int(match[1]),
            "sha256": match[2],
        }
result = {
    "analysis_rows": len(analysis),
    "load_rows": len(loads),
    "run_order_and_tokens_valid": True,
    "all_summary_statistics_recomputed": True,
    "measured_source_hashes_verified": True,
    "samples_excluded": 0,
    "all_field_comparisons": comparisons,
    "system_cpu_percent_during_analysis_process": {
        "min": min(utilization),
        "median": percentile(utilization, 0.5),
        "max": max(utilization),
    },
    "paired_medians_by_order": orders,
}
(ROOT / "audit.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result, indent=2))
