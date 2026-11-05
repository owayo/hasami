"""Verify and benchmark v5 versus the isolated symmetric-entry reader.

Usage: python docs/measurements/hsd-symmetric-20261105/run.py TRIAL
Options: --verify-only, --timings-only. Timing outputs are never overwritten.
"""

import argparse
import hashlib
import importlib.util
import json
import os
import platform
import subprocess
import sys
import zipfile
from pathlib import Path

NAMES = ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"]
RUNTIME = [
    "src/hsd/container.rs",
    "src/hsd/records.rs",
    "src/hsd/reader.rs",
    "src/hsd/writer.rs",
    "src/hsd/mod.rs",
    "src/lattice.rs",
    "src/analyzer/contextual_readings.rs",
    "src/hsd/tests.rs",
]


def fingerprint(path):
    """Fingerprint bytes without keeping the file in memory.

    Returns:
        A byte count and SHA-256.

    """
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"bytes": path.stat().st_size, "sha256": digest}


def write_json(path, data):
    """Write a UTF-8/LF structured record."""
    path.write_text(
        json.dumps(data, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def main(trial, verify_only, timings_only):
    """Verify identities and outputs before any timed runs."""
    repo = Path.cwd()
    output = trial / "results"
    output.mkdir(exist_ok=True)
    assert not (output / "analysis.jsonl").exists(), (
        "use a new trial for a new timing series"
    )
    spec = importlib.util.spec_from_file_location(
        "bench", repo / "scripts/hsd-pronunciation-remeasure.py"
    )
    bench = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(bench)
    bench.VARIANTS = {
        "v5": (trial / "v5", trial / "eval-v5.exe"),
        "patch": (trial / "symmetric", trial / "eval-symmetric.exe"),
    }
    inputs = json.loads((trial / "input-manifest.json").read_text(encoding="utf-8"))
    manifest = {
        **inputs,
        "platform": platform.platform(),
        "rounds": 12,
        "affinity_mask": 4,
        "format": {"v5": 5, "patch": 5013},
        "memory": "Windows PeakWorkingSetSize of the whole process",
        "cache": "warm pages, one warmup pass per analysis process",
        "executables": {},
        "runtime_sources": {},
        "measurement_sources": {},
    }
    for variant, (directory, exe) in bench.VARIANTS.items():
        manifest["executables"][variant] = fingerprint(exe)
        for name in NAMES:
            label = "v5" if variant == "v5" else "symmetric"
            assert (
                fingerprint(directory / f"{name}.hsd")
                == inputs["dictionaries"][name][label]
            )
    for variant in ["v5", "symmetric"]:
        for relative in RUNTIME:
            manifest["runtime_sources"][f"{variant}/{relative}"] = fingerprint(
                trial / f"{variant}-source" / relative
            )
    extra_test = trial / "symmetric-source/src/hsd/symmetric_tests.rs"
    manifest["runtime_sources"]["symmetric/src/hsd/symmetric_tests.rs"] = fingerprint(
        extra_test
    )
    sources = [
        repo / "scripts/hsd-format-eval.rs",
        repo / "scripts/hsd-format-compare.py",
        repo / "scripts/hsd-pronunciation-remeasure.py",
        Path(__file__).resolve(),
        Path(__file__).with_name("prepare.py").resolve(),
        Path(__file__).with_name("design.md").resolve(),
    ]
    for path in sources:
        manifest["measurement_sources"][path.relative_to(repo).as_posix()] = (
            fingerprint(path)
        )
    for name in ["mixed", "short", "literature", "quantities"]:
        assert fingerprint(trial / "corpus" / f"{name}.txt") == {
            key: inputs["corpora"][name][key] for key in ["bytes", "sha256"]
        }
    os.environ["HASAMI_BENCH_AFFINITY"] = "4"
    if timings_only:
        old = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
        for field in [
            "dictionaries",
            "corpora",
            "executables",
            "runtime_sources",
            "measurement_sources",
            "rounds",
            "affinity_mask",
            "format",
            "cache",
            "memory",
            "baseline_head",
            "platform",
        ]:
            assert old[field] == manifest[field], field
        for name in NAMES:
            for corpus in ["mixed", "short", "literature", "quantities"]:
                assert "equal bytes=" in (
                    output / f"compare-{corpus}-{name}.txt"
                ).read_text(encoding="utf-8")
    else:
        assert not (output / "analysis.jsonl").exists()
        for name in NAMES:
            for variant, (directory, exe) in bench.VARIANTS.items():
                with (output / f"verify-{variant}-{name}.txt").open(
                    "w", encoding="utf-8"
                ) as log:
                    subprocess.run(
                        [str(exe), "verify", str(directory / f"{name}.hsd")],
                        stdout=log,
                        stderr=subprocess.STDOUT,
                        check=True,
                    )
            for corpus in ["mixed", "short", "literature", "quantities"]:
                with (output / f"compare-{corpus}-{name}.txt").open(
                    "w", encoding="utf-8"
                ) as log:
                    subprocess.run(
                        [
                            sys.executable,
                            str(repo / "scripts/hsd-format-compare.py"),
                            str(trial / "eval-v5.exe"),
                            str(trial / "v5" / f"{name}.hsd"),
                            str(trial / "eval-symmetric.exe"),
                            str(trial / "symmetric" / f"{name}.hsd"),
                            str(trial / "corpus" / f"{corpus}.txt"),
                        ],
                        stdout=log,
                        stderr=subprocess.STDOUT,
                        check=True,
                    )
                print(f"all fields equal: {name} {corpus}", flush=True)
    write_json(output / "manifest.json", manifest)
    if verify_only:
        return
    assert not (output / "analysis.jsonl").exists(), (
        "use a new trial for a new timing series"
    )
    with zipfile.ZipFile(
        output / "measured-sources.zip", "w", zipfile.ZIP_DEFLATED
    ) as archive:
        for path in sources:
            archive.writestr(path.relative_to(repo).as_posix(), path.read_bytes())
        for variant in ["v5", "symmetric"]:
            for relative in RUNTIME:
                archive.writestr(
                    f"{variant}/{relative}",
                    (trial / f"{variant}-source" / relative).read_bytes(),
                )
        archive.writestr(
            "symmetric/src/hsd/symmetric_tests.rs", extra_test.read_bytes()
        )
    analysis = []
    loads = []
    with (output / "analysis.jsonl").open(
        "x", encoding="utf-8", newline="\n"
    ) as records:
        for corpus in ["mixed", "short", "literature"]:
            for turn in range(12):
                names = NAMES[turn % 3 :] + NAMES[: turn % 3]
                order = ["v5", "patch"] if turn % 2 == 0 else ["patch", "v5"]
                for name in names:
                    for position, variant in enumerate(order):
                        directory, exe = bench.VARIANTS[variant]
                        stdout, telemetry = bench.run_eval(
                            exe,
                            "bench",
                            directory / f"{name}.hsd",
                            output / f"time-{corpus}-{name}-{turn}-{variant}.txt",
                            trial / "corpus" / f"{corpus}.txt",
                        )
                        row = {
                            "corpus": corpus,
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
                print(f"timed {corpus}: round {turn + 1}/12", flush=True)
    with (output / "load.jsonl").open("x", encoding="utf-8", newline="\n") as records:
        for turn in range(12):
            for name in NAMES[turn % 3 :] + NAMES[: turn % 3]:
                order = ["v5", "patch"] if turn % 2 == 0 else ["patch", "v5"]
                for position, variant in enumerate(order):
                    directory, exe = bench.VARIANTS[variant]
                    stdout, telemetry = bench.run_eval(
                        exe,
                        "load",
                        directory / f"{name}.hsd",
                        output / f"time-load-{name}-{turn}-{variant}.txt",
                    )
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
    write_json(output / "summary.json", bench.summarize(analysis, loads))
    singles = []
    for turn in range(12):
        for name in NAMES[turn % 3 :] + NAMES[: turn % 3]:
            order = ["v5", "patch"] if turn % 2 == 0 else ["patch", "v5"]
            for variant in order:
                directory, exe = bench.VARIANTS[variant]
                stdout, telemetry = bench.run_eval(
                    exe,
                    "load-once",
                    directory / f"{name}.hsd",
                    output / f"time-single-{name}-{turn}-{variant}.txt",
                )
                values = dict(part.split("=", 1) for part in stdout.split()[2:])
                singles.append(
                    {
                        "round": turn,
                        "dictionary": name,
                        "variant": variant,
                        **telemetry,
                        "wall_us": float(values["wall_us"]),
                        "cpu_us": float(values["cpu_us"]),
                        "tokens": int(values["tokens"]),
                    }
                )
    (output / "load-once.jsonl").write_text(
        "".join(json.dumps(row) + "\n" for row in singles),
        encoding="utf-8",
        newline="\n",
    )
    print("Finished all timed analysis and load comparisons.", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trial", type=Path)
    parser.add_argument("--verify-only", action="store_true")
    parser.add_argument("--timings-only", action="store_true")
    args = parser.parse_args()
    main(args.trial.resolve(), args.verify_only, args.timings_only)
