"""Repeat the pronunciation experiment with interleaved analysis and load runs.

Usage: python hsd-pronunciation-remeasure.py OUTPUT V5_DICTS PATCH_DICTS
       V5_EVAL PATCH_EVAL CORPUS [ROUNDS] [LITERATURE_CORPUS]
Build both evals from the same hsd-format-eval.rs before running this script.
"""

import json
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path

NAMES = ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"]
VARIANTS = {}


def windows_system_times():
    """Snapshot Windows CPU accounting.

    Returns:
        System idle, kernel (including idle), and user ticks.

    Raises:
        OSError: If GetSystemTimes fails.
    """
    import ctypes
    from ctypes import wintypes

    api = ctypes.WinDLL("kernel32", use_last_error=True)
    api.GetSystemTimes.argtypes = [ctypes.POINTER(wintypes.FILETIME)] * 3
    api.GetSystemTimes.restype = wintypes.BOOL
    values = [wintypes.FILETIME() for _ in range(3)]
    if not api.GetSystemTimes(*(ctypes.byref(value) for value in values)):
        raise ctypes.WinError(ctypes.get_last_error())
    return [(value.dwHighDateTime << 32) | value.dwLowDateTime for value in values]


def run_eval(exe, mode, dictionary, log, corpus=None):
    """Run one eval process and collect its resource usage and host load."""
    windows = sys.platform == "win32"
    command = [str(exe), mode, str(dictionary)]
    if not windows:
        command = ["/usr/bin/time", "-l", *command]
    if corpus is not None:
        command.extend([str(corpus), "1"])
    before = windows_system_times() if windows else os.getloadavg()
    with log.open("w") as err:
        run = subprocess.run(
            command, text=True, stdout=subprocess.PIPE, stderr=err, check=True
        )
    after = windows_system_times() if windows else os.getloadavg()
    if windows:
        peak = next(
            int(line.split("=", 1)[1])
            for line in log.read_text().splitlines()
            if line.startswith("peak_working_set_bytes=")
        )
        idle, kernel, user = [b - a for a, b in zip(before, after, strict=True)]
        total = kernel + user
        return run.stdout, {
            "rss_bytes": peak,
            "memory_metric": "Windows PeakWorkingSetSize (whole process)",
            "system_cpu_percent_during_process": (1 - idle / total) * 100
            if total
            else None,
            "finished_unix_seconds": time.time(),
        }
    rss = next(
        int(line.split()[0])
        for line in log.read_text().splitlines()
        if "maximum resident set size" in line
    )
    return run.stdout, {
        "rss_bytes": rss,
        "load_average_before": before,
        "load_average_after": os.getloadavg(),
        "finished_unix_seconds": time.time(),
    }


def describe(values):
    """Return the sample count, median, and full range without filtering."""
    quartiles = statistics.quantiles(values, n=4, method="inclusive")
    mean = statistics.mean(values)
    return {
        "n": len(values),
        "median": statistics.median(values),
        "min": min(values),
        "max": max(values),
        "iqr": quartiles[2] - quartiles[0],
        "cv": statistics.pstdev(values) / mean if mean else None,
    }


def summarize(analysis, loads):
    """Summarize analysis pairs and separate first loads from reconstructions."""
    summary = {"analysis": {}, "load": {}}
    for corpus in dict.fromkeys(row["corpus"] for row in analysis):
        summary["analysis"][corpus] = {}
        for name in NAMES:
            rows = [
                row
                for row in analysis
                if row["dictionary"] == name and row["corpus"] == corpus
            ]
            result = {}
            for variant in VARIANTS:
                group = [row for row in rows if row["variant"] == variant]
                result[variant] = {
                    field: describe([row[field] for row in group])
                    for field in ["cpu", "wall", "rss_bytes"]
                }
            pairs = []
            for turn in sorted({row["round"] for row in rows}):
                pair = {row["variant"]: row for row in rows if row["round"] == turn}
                pairs.append(pair["patch"]["cpu"] / pair["v5"]["cpu"] - 1)
            result["paired_cpu_relative_change"] = describe(pairs)
            result["paired_cpu_relative_changes"] = pairs
            result["cpu_median_relative_change"] = (
                result["patch"]["cpu"]["median"] / result["v5"]["cpu"]["median"] - 1
            )
            result["pairs_patch_slower"] = sum(value > 0 for value in pairs)
            wall_pairs = []
            for turn in sorted({row["round"] for row in rows}):
                pair = {row["variant"]: row for row in rows if row["round"] == turn}
                wall_pairs.append(pair["patch"]["wall"] / pair["v5"]["wall"] - 1)
            result["paired_wall_relative_change"] = describe(wall_pairs)
            result["paired_wall_relative_changes"] = wall_pairs
            result["wall_median_relative_change"] = (
                result["patch"]["wall"]["median"] / result["v5"]["wall"]["median"] - 1
            )
            result["pairs_patch_wall_slower"] = sum(value > 0 for value in wall_pairs)
            summary["analysis"][corpus][name] = result
    for name in NAMES:
        summary["load"][name] = {}
        for variant in VARIANTS:
            rows = [
                row
                for row in loads
                if row["dictionary"] == name and row["variant"] == variant
            ]
            # The first constructor in each process is reported separately.
            summary["load"][name][variant] = {
                scope: {
                    field: describe(
                        [row[field] for row in rows if (row["sample"] == 0) == first]
                    )
                    for field in ["wall_us", "cpu_us"]
                }
                for scope, first in [
                    ("first_in_process", True),
                    ("reconstructed", False),
                ]
            }
    return summary


if __name__ == "__main__":
    if sys.platform not in ("darwin", "win32"):
        raise SystemExit("This runner requires macOS or Windows and the matching eval")
    if len(sys.argv) not in (7, 8, 9):
        raise SystemExit(__doc__)
    output, old, new, old_eval, new_eval, corpus = (
        Path(value).resolve() for value in sys.argv[1:7]
    )
    rounds = int(sys.argv[7]) if len(sys.argv) >= 8 else 12
    if rounds < 2 or rounds % 2:
        raise SystemExit("Use an even number of rounds, at least two, to balance order")
    output.mkdir(parents=True, exist_ok=False)
    VARIANTS = {"v5": (old, old_eval), "patch": (new, new_eval)}
    short = output / "short.txt"
    short.write_text(
        "東京都に住んでいる人々が増えている。\n" * 100_000,
        encoding="utf-8",
        newline="\n",
    )
    corpora = [("mixed", corpus), ("short", short)]
    if len(sys.argv) == 9:
        corpora.append(("literature", Path(sys.argv[8]).resolve()))
    analysis = []
    loads = []
    with (output / "analysis.jsonl").open("w") as records:
        for corpus_name, text in corpora:
            for turn in range(rounds):
                names = NAMES[turn % 3 :] + NAMES[: turn % 3]
                for name in names:
                    order = ["v5", "patch"] if turn % 2 == 0 else ["patch", "v5"]
                    for position, variant in enumerate(order):
                        root, exe = VARIANTS[variant]
                        log = output / f"time-{corpus_name}-{name}-{turn}-{variant}.txt"
                        stdout, telemetry = run_eval(
                            exe, "bench", root / f"{name}.hsd", log, text
                        )
                        row = {
                            "corpus": corpus_name,
                            "dictionary": name,
                            "round": turn,
                            "position_in_pair": position,
                            "variant": variant,
                            **telemetry,
                        }
                        for field in stdout.split():
                            key, value = field.split("=")
                            row[key] = (
                                float(value) if key in {"wall", "cpu"} else int(value)
                            )
                        analysis.append(row)
                        records.write(json.dumps(row) + "\n")
                        records.flush()
                        print(json.dumps(row), flush=True)
    with (output / "load.jsonl").open("w") as records:
        for turn in range(rounds):
            names = NAMES[turn % 3 :] + NAMES[: turn % 3]
            for name in names:
                order = ["v5", "patch"] if turn % 2 == 0 else ["patch", "v5"]
                for position, variant in enumerate(order):
                    root, exe = VARIANTS[variant]
                    log = output / f"time-load-{name}-{turn}-{variant}.txt"
                    stdout, telemetry = run_eval(exe, "load", root / f"{name}.hsd", log)
                    for line in stdout.splitlines():
                        _, sample, *fields = line.split()
                        row = {
                            "dictionary": name,
                            "variant": variant,
                            "round": turn,
                            "position_in_pair": position,
                            "sample": int(sample),
                            **telemetry,
                        }
                        row.update(
                            {k: float(v) for k, v in (f.split("=") for f in fields)}
                        )
                        loads.append(row)
                        records.write(json.dumps(row) + "\n")
                    records.flush()
    (output / "summary.json").write_text(
        json.dumps(summarize(analysis, loads), indent=2) + "\n"
    )
