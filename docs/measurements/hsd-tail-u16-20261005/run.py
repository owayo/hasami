"""Verify and measure the isolated u16 TAIL experiment on Windows.

Usage: mise exec -- python docs/measurements/hsd-tail-u16-20261005/run.py TRIAL
TRIAL must contain the two built evaluators and u16 dictionaries. The existing
hsd-repeat-windows inputs are hash-checked against the saved prior manifest.
No build, conversion, or dump comparison overlaps the timed measurements.
"""

import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path


def _fingerprint(path):
    """Get the size and SHA-256 of a file without decoding its contents.

    Returns:
        A mapping with byte count and SHA-256.

    """
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"bytes": path.stat().st_size, "sha256": digest}


trial = Path(sys.argv[1]).resolve()
repo = Path(__file__).resolve().parents[3]
record = Path(__file__).resolve().parent
inputs = repo / "target/hsd-repeat-windows"
prior = json.loads(
    (
        repo / "docs/measurements/hsd-pronunciation-20261005-windows/manifest.json"
    ).read_text(encoding="utf-8")
)
names = ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"]
corpus_paths = {
    name: inputs / "corpus" / f"{name}.txt" for name in ["mixed", "literature"]
}
corpus_paths["short"] = trial / "short.txt"
corpus_paths["short"].write_text(
    (prior["corpora"]["short"]["text"] + "\n") * 100000,
    encoding="utf-8",
    newline="\n",
)
manifest = {
    "date_jst": "2026-10-05",
    "baseline_commit": "4de1d02",
    "format": {"v5": 5, "patch": 5007},
    "variant_labels": {"v5": "v5 UTF-8 TAIL", "patch": "u16-code TAIL"},
    "platform": {**prior["platform"], "os": platform.platform()},
    "toolchain": prior["toolchain"],
    "dictionaries": {},
    "corpora": {},
    "executables": {},
    "sources": {},
    "conditions": {
        "rounds": 12,
        "affinity_mask_decimal": 4,
        "primary_clock": "Rust Instant wall",
        "cpu_clock": "GetProcessTimes kernel+user; coarse supplementary value",
        "cache": "warm OS pages; one warmup pass per analysis process",
        "memory": "Windows PeakWorkingSetSize, entire process",
        "order": "AB/BA alternating, dictionary order rotates",
        "single_load": "12 new processes per dictionary and variant, one load+sentence",
    },
}
for name in names:
    old = _fingerprint(inputs / "v5" / f"{name}.hsd")
    assert old == prior["dictionaries"][name]["v5"]
    manifest["dictionaries"][name] = {
        "v5": old,
        "patch": _fingerprint(trial / "u16" / f"{name}.hsd"),
    }
for name in ["mixed", "short", "literature"]:
    path = corpus_paths[name]
    actual = _fingerprint(path)
    assert actual == {key: prior["corpora"][name][key] for key in actual}
    manifest["corpora"][name] = prior["corpora"][name]
for variant in ["v5", "u16"]:
    manifest["executables"][variant] = _fingerprint(trial / f"eval-{variant}.exe")
source_paths = [
    repo / "scripts/hsd-format-eval.rs",
    repo / "scripts/hsd-format-compare.py",
    repo / "scripts/hsd-pronunciation-remeasure.py",
    repo / "scripts/hsd-tail-u16-probe.py",
    record / "reader.patch",
    Path(__file__).resolve(),
]
for path in source_paths:
    manifest["sources"][str(path.relative_to(repo)).replace("\\", "/")] = _fingerprint(
        path
    )
for variant in ["v5", "u16"]:
    for name in ["trie.rs", "container.rs"]:
        path = trial / f"{variant}-source/src/hsd" / name
        manifest["sources"][f"{variant}-source/{name}"] = _fingerprint(path)
results = trial / "results"
results.mkdir(exist_ok=True)
reuse_verified = "--reuse-verified" in sys.argv[2:]
if reuse_verified:
    old_manifest = json.loads((results / "manifest.json").read_text(encoding="utf-8"))
    for field in ["dictionaries", "corpora", "executables"]:
        assert old_manifest[field] == manifest[field]
    for name in names:
        for variant in ["v5", "u16"]:
            assert (
                "VerifyReport" in (results / f"verify-{variant}-{name}.txt").read_text()
            )
        for corpus in ["mixed", "short", "literature"]:
            assert re.search(
                r"equal bytes=\d+ sha256=[0-9a-f]{64}",
                (results / f"compare-{corpus}-{name}.txt").read_text(),
            )
(results / "manifest.json").write_text(
    json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
    encoding="utf-8",
    newline="\n",
)
with zipfile.ZipFile(
    results / "measured-sources.zip", "w", zipfile.ZIP_DEFLATED
) as archive:
    for path in source_paths:
        archive.write(path, str(path.relative_to(repo)).replace("\\", "/"))
    for variant in ["v5", "u16"]:
        for name in ["trie.rs", "container.rs"]:
            archive.write(
                trial / f"{variant}-source/src/hsd" / name,
                f"{variant}-source/{name}",
            )

os.environ["HASAMI_BENCH_AFFINITY"] = "4"
for name in [] if reuse_verified else names:
    for variant, directory in [("v5", inputs / "v5"), ("u16", trial / "u16")]:
        with (results / f"verify-{variant}-{name}.txt").open(
            "w", encoding="utf-8"
        ) as log:
            subprocess.run(
                [
                    str(trial / f"eval-{variant}.exe"),
                    "verify",
                    str(directory / f"{name}.hsd"),
                ],
                stdout=log,
                stderr=subprocess.STDOUT,
                check=True,
            )
        print(f"verify {variant} {name}: OK", flush=True)
    for corpus in ["mixed", "short", "literature"]:
        with (results / f"compare-{corpus}-{name}.txt").open(
            "w", encoding="utf-8"
        ) as log:
            subprocess.run(
                [
                    sys.executable,
                    str(repo / "scripts/hsd-format-compare.py"),
                    str(trial / "eval-v5.exe"),
                    str(inputs / "v5" / f"{name}.hsd"),
                    str(trial / "eval-u16.exe"),
                    str(trial / "u16" / f"{name}.hsd"),
                    str(corpus_paths[corpus]),
                ],
                stdout=log,
                stderr=subprocess.STDOUT,
                check=True,
            )
        print(f"all-field comparison {corpus} {name}: OK", flush=True)

subprocess.run(
    [
        sys.executable,
        str(repo / "scripts/hsd-pronunciation-remeasure.py"),
        str(trial / "timings"),
        str(inputs / "v5"),
        str(trial / "u16"),
        str(trial / "eval-v5.exe"),
        str(trial / "eval-u16.exe"),
        str(inputs / "corpus/mixed.txt"),
        "12",
        str(inputs / "corpus/literature.txt"),
    ],
    check=True,
)
for filename in ["analysis.jsonl", "load.jsonl", "summary.json"]:
    shutil.copyfile(trial / "timings" / filename, results / filename)
resource_logs = [
    {"file": path.name, "stderr": path.read_text()}
    for path in sorted((trial / "timings").glob("time-*.txt"))
]
(results / "resource-logs.jsonl").write_text(
    "".join(json.dumps(row) + "\n" for row in resource_logs),
    encoding="utf-8",
    newline="\n",
)
# This stage runs only after all analysis/constructor timings have finished.
rows = []
for turn in range(12):
    order = ["v5", "u16"] if turn % 2 == 0 else ["u16", "v5"]
    rotated = names[turn % 3 :] + names[: turn % 3]
    for name in rotated:
        for variant in order:
            directory = inputs / "v5" if variant == "v5" else trial / "u16"
            completed = subprocess.run(
                [
                    str(trial / f"eval-{variant}.exe"),
                    "load-once",
                    str(directory / f"{name}.hsd"),
                ],
                text=True,
                capture_output=True,
                check=True,
            )
            values = dict(part.split("=", 1) for part in completed.stdout.split()[2:])
            peak = int(completed.stderr.strip().split("=", 1)[1])
            rows.append(
                {
                    "round": turn,
                    "dictionary": name,
                    "variant": variant,
                    "wall_us": float(values["wall_us"]),
                    "cpu_us": float(values["cpu_us"]),
                    "tokens": int(values["tokens"]),
                    "peak_working_set_bytes": peak,
                }
            )
    print(f"single-load round {turn + 1}/12", flush=True)
(results / "load-once.jsonl").write_text(
    "".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8", newline="\n"
)
print("all measurements complete", flush=True)
