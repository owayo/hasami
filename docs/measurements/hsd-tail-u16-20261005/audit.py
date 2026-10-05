"""Independently recompute statistics and check all saved experiment records.

Usage: mise exec -- python docs/measurements/hsd-tail-u16-20261005/audit.py [RESULTS]
"""

import hashlib
import json
import math
import re
import sys
import zipfile
from pathlib import Path

ROOT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent
NAMES = ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"]
CORPORA = ["mixed", "short", "literature"]
TOKENS = {
    "mixed": [5183475, 4859437, 4852226],
    "short": [1200000, 1100000, 1100000],
    "literature": [5711900, 5622300, 5579900],
}


def _percentile(values, fraction):
    """Calculate a linearly interpolated percentile.

    Returns:
        The interpolated sample value.

    """
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def _stats(values):
    """Compute descriptive statistics without using the runner's implementation.

    Returns:
        Count, median, range, IQR, and coefficient of variation.

    """
    mean = sum(values) / len(values)
    return {
        "n": len(values),
        "median": _percentile(values, 0.5),
        "min": min(values),
        "max": max(values),
        "iqr": _percentile(values, 0.75) - _percentile(values, 0.25),
        "cv": math.sqrt(sum((value - mean) ** 2 for value in values) / len(values))
        / mean
        if mean
        else None,
    }


def _check_stats(actual, values):
    """Assert that saved statistics agree with independent calculations."""
    for key, value in _stats(values).items():
        assert (
            actual[key] is None
            if value is None
            else math.isclose(actual[key], value, rel_tol=1e-9, abs_tol=1e-9)
        ), (key, actual[key], value)


def _rows(name):
    """Read newline-delimited JSON records.

    Returns:
        The decoded records in original run order.

    """
    return [
        json.loads(line)
        for line in (ROOT / name).read_text(encoding="utf-8").splitlines()
    ]


analysis = _rows("analysis.jsonl")
loads = _rows("load.jsonl")
once = _rows("load-once.jsonl")
summary = json.loads((ROOT / "summary.json").read_text(encoding="utf-8"))
manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
with zipfile.ZipFile(ROOT / "measured-sources.zip") as archive:
    for name, identity in manifest["sources"].items():
        data = archive.read(name)
        assert len(data) == identity["bytes"]
        assert hashlib.sha256(data).hexdigest() == identity["sha256"]
expected = []
expected_loads = []
expected_once = []
for corpus in CORPORA:
    for turn in range(12):
        names = NAMES[turn % 3 :] + NAMES[: turn % 3]
        for name in names:
            order = ["v5", "patch"] if turn % 2 == 0 else ["patch", "v5"]
            for position, variant in enumerate(order):
                expected.append((corpus, turn, name, position, variant))
                if corpus == "mixed":
                    expected_loads.extend(
                        (turn, name, position, variant, sample) for sample in range(31)
                    )
                    expected_once.append(
                        (turn, name, "u16" if variant == "patch" else variant)
                    )
assert len(analysis) == 216 and len(loads) == 2232 and len(once) == 72
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
assert [
    tuple(row[key] for key in ["round", "dictionary", "variant"]) for row in once
] == expected_once
for row in analysis:
    assert row["tokens"] == TOKENS[row["corpus"]][NAMES.index(row["dictionary"])]
    assert row["wall"] > 0 and row["cpu"] > 0 and row["rss_bytes"] > 0

orders = {}
for corpus in CORPORA:
    orders[corpus] = {}
    for name in NAMES:
        rows = [
            row
            for row in analysis
            if row["corpus"] == corpus and row["dictionary"] == name
        ]
        saved = summary["analysis"][corpus][name]
        for variant in ["v5", "patch"]:
            group = [row for row in rows if row["variant"] == variant]
            for field in ["wall", "cpu", "rss_bytes"]:
                _check_stats(saved[variant][field], [row[field] for row in group])
        orders[corpus][name] = {}
        for field in ["wall", "cpu"]:
            ratios = []
            for turn in range(12):
                pair = {row["variant"]: row for row in rows if row["round"] == turn}
                ratios.append(pair["patch"][field] / pair["v5"][field] - 1)
            _check_stats(saved[f"paired_{field}_relative_change"], ratios)
            assert saved[f"paired_{field}_relative_changes"] == ratios
            assert saved[
                "pairs_patch_wall_slower" if field == "wall" else "pairs_patch_slower"
            ] == sum(ratio > 0 for ratio in ratios)
            assert math.isclose(
                saved[f"{field}_median_relative_change"],
                saved["patch"][field]["median"] / saved["v5"][field]["median"] - 1,
            )
            orders[corpus][name][field] = {
                "AB_median": _percentile(ratios[::2], 0.5),
                "BA_median": _percentile(ratios[1::2], 0.5),
            }
for name in NAMES:
    for variant in ["v5", "patch"]:
        rows = [
            row
            for row in loads
            if row["dictionary"] == name and row["variant"] == variant
        ]
        for scope, first in [("first_in_process", True), ("reconstructed", False)]:
            for field in ["wall_us", "cpu_us"]:
                _check_stats(
                    summary["load"][name][variant][scope][field],
                    [row[field] for row in rows if (row["sample"] == 0) == first],
                )

single = {}
for name in NAMES:
    single[name] = {}
    rows = [row for row in once if row["dictionary"] == name]
    for variant in ["v5", "u16"]:
        group = [row for row in rows if row["variant"] == variant]
        assert all(
            row["tokens"] == TOKENS["short"][NAMES.index(name)] / 100000
            for row in group
        )
        single[name][variant] = {
            field: _stats([row[field] for row in group])
            for field in ["wall_us", "cpu_us", "peak_working_set_bytes"]
        }
    ratios = []
    for turn in range(12):
        pair = {row["variant"]: row for row in rows if row["round"] == turn}
        ratios.append(pair["u16"]["wall_us"] / pair["v5"]["wall_us"] - 1)
    single[name]["paired_wall_relative_change"] = _stats(ratios)

comparisons = {}
for corpus in CORPORA:
    comparisons[corpus] = {}
    for name in NAMES:
        text = (ROOT / f"compare-{corpus}-{name}.txt").read_text(encoding="utf-8")
        match = re.search(r"equal bytes=(\d+) sha256=([0-9a-f]{64})", text)
        assert match, (corpus, name)
        comparisons[corpus][name] = {
            "equal_bytes": int(match[1]),
            "sha256": match[2],
        }
utilization = [row["system_cpu_percent_during_process"] for row in analysis]
result = {
    "analysis_rows": len(analysis),
    "load_rows": len(loads),
    "load_once_rows": len(once),
    "run_order_and_tokens_valid": True,
    "all_summary_statistics_recomputed": True,
    "measured_source_hashes_verified": True,
    "samples_excluded": 0,
    "all_field_comparisons": comparisons,
    "system_cpu_percent_during_analysis_process": _stats(utilization),
    "paired_medians_by_order": orders,
    "load_once": single,
}
(ROOT / "audit.json").write_text(
    json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n"
)
print(json.dumps(result, indent=2))
