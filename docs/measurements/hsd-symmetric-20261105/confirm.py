"""Confirm the noisy recommended-dictionary literary case with longer timing.

Usage: python docs/measurements/hsd-symmetric-20261105/confirm.py TRIAL
Twelve AB/BA pairs, eight measured passes per process; no input or binary changes.
"""

import hashlib
import importlib.util
import json
import os
import statistics
import subprocess
import sys
import zipfile
from pathlib import Path


def fingerprint(path):
    """Return content identity.

    Returns:
        The file's byte length and SHA-256.

    """
    return {
        "bytes": path.stat().st_size,
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
    }


def main(trial):
    """Record a separate confirmation series without overwriting the initial run."""
    repo = Path.cwd()
    prior = trial / "results"
    manifest = json.loads((prior / "manifest.json").read_text(encoding="utf-8"))
    name = "ipadic-neologd-sudachi"
    corpus = trial / "corpus/literature.txt"
    assert fingerprint(corpus) == {
        k: manifest["corpora"]["literature"][k] for k in ["bytes", "sha256"]
    }
    for variant, directory in [("v5", "v5"), ("patch", "symmetric")]:
        assert (
            fingerprint(trial / f"eval-{directory}.exe")
            == manifest["executables"][variant]
        )
        assert (
            fingerprint(trial / directory / f"{name}.hsd")
            == manifest["dictionaries"][name][directory]
        )
    for path in [
        "scripts/hsd-format-eval.rs",
        "scripts/hsd-format-compare.py",
        "scripts/hsd-pronunciation-remeasure.py",
    ]:
        assert fingerprint(repo / path) == manifest["measurement_sources"][path]
    assert "equal bytes=" in (prior / f"compare-literature-{name}.txt").read_text(
        encoding="utf-8"
    )
    output = trial / "confirmation"
    output.mkdir(exist_ok=False)
    spec = importlib.util.spec_from_file_location(
        "bench", repo / "scripts/hsd-pronunciation-remeasure.py"
    )
    bench = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(bench)
    os.environ["HASAMI_BENCH_AFFINITY"] = "4"
    rows = []
    with (output / "analysis.jsonl").open("w", encoding="utf-8", newline="\n") as log:
        for turn in range(12):
            order = ["v5", "patch"] if turn % 2 == 0 else ["patch", "v5"]
            for position, variant in enumerate(order):
                directory = "v5" if variant == "v5" else "symmetric"
                before = bench.windows_system_times()
                result = subprocess.run(
                    [
                        str(trial / f"eval-{directory}.exe"),
                        "bench",
                        str(trial / directory / f"{name}.hsd"),
                        str(corpus),
                        "8",
                    ],
                    text=True,
                    capture_output=True,
                    check=True,
                )
                after = bench.windows_system_times()
                passes = []
                for line in result.stdout.splitlines():
                    fields = dict(value.split("=", 1) for value in line.split())
                    passes.append(
                        {
                            key: float(value) if key in {"wall", "cpu"} else int(value)
                            for key, value in fields.items()
                        }
                    )
                assert len(passes) == 8 and [r["pass"] for r in passes] == list(
                    range(8)
                )
                assert len({(r["lines"], r["tokens"]) for r in passes}) == 1
                idle, kernel, user = [b - a for a, b in zip(before, after, strict=True)]
                row = {
                    "dictionary": name,
                    "corpus": "literature",
                    "round": turn,
                    "position_in_pair": position,
                    "variant": variant,
                    "wall": statistics.mean(r["wall"] for r in passes),
                    "cpu": statistics.mean(r["cpu"] for r in passes),
                    "rss_bytes": int(result.stderr.strip().split("=", 1)[1]),
                    "passes": passes,
                    "system_cpu_percent_during_process": (1 - idle / (kernel + user))
                    * 100,
                }
                rows.append(row)
                log.write(json.dumps(row) + "\n")
                log.flush()
            print(f"long confirmation: pair {turn + 1}/12", flush=True)
    pairs = []
    for turn in range(12):
        pair = {r["variant"]: r for r in rows if r["round"] == turn}
        assert {
            (p["lines"], p["tokens"]) for r in pair.values() for p in r["passes"]
        }.__len__() == 1
        pairs.append(pair["patch"]["wall"] / pair["v5"]["wall"] - 1)
    medians = {
        v: statistics.median(r["wall"] for r in rows if r["variant"] == v)
        for v in ["v5", "patch"]
    }
    summary = {
        "measured_passes_per_process": 8,
        "pairs": 12,
        "wall_medians": medians,
        "wall_median_relative_change": medians["patch"] / medians["v5"] - 1,
        "paired_relative_changes": pairs,
        "paired_median": statistics.median(pairs),
        "order_medians": {
            "AB": statistics.median(pairs[::2]),
            "BA": statistics.median(pairs[1::2]),
        },
        "slower_pairs": sum(p > 0 for p in pairs),
        "source": fingerprint(Path(__file__)),
        "corpus": fingerprint(corpus),
        "executables": manifest["executables"],
    }
    (output / "summary.json").write_text(
        json.dumps(summary, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    with zipfile.ZipFile(output / "sources.zip", "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("confirm.py", Path(__file__).read_bytes())
    print("Long confirmation finished.", flush=True)


if __name__ == "__main__":
    main(Path(sys.argv[1]).resolve())
