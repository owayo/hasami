"""Compare the experimental reader with v5 on macOS after other jobs finish.

Usage: python hsd-pronunciation-evaluate.py OUTPUT V5_DICTS PATCH_DICTS
       V5_EVAL PATCH_EVAL CORPUS [ROUNDS]
The eval executables are built from hsd-format-eval.rs against each library.
"""

import json
import subprocess
import sys
from pathlib import Path

output, old, new, old_eval, new_eval, corpus = map(Path, sys.argv[1:7])
rounds = int(sys.argv[7]) if len(sys.argv) > 7 else 6
if sys.platform != "darwin":
    raise SystemExit("This runner uses macOS time -l and the eval's macOS CPU clock")
output.mkdir(parents=True, exist_ok=True)
variants = {"v5": (old, old_eval), "patch": (new, new_eval)}
with (output / "analysis.jsonl").open("w") as records:
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"]:
        for turn in range(rounds):
            order = ["v5", "patch"] if turn % 2 == 0 else ["patch", "v5"]
            for variant in order:
                root, exe = variants[variant]
                log = output / f"time-{name}-{turn}-{variant}.txt"
                with log.open("w") as err:
                    run = subprocess.run(
                        [
                            "/usr/bin/time",
                            "-l",
                            str(exe),
                            "bench",
                            str(root / f"{name}.hsd"),
                            str(corpus),
                            "1",
                        ],
                        text=True,
                        stdout=subprocess.PIPE,
                        stderr=err,
                        check=True,
                    )
                row = {"dictionary": name, "round": turn, "variant": variant}
                for field in run.stdout.split():
                    key, value = field.split("=")
                    row[key] = float(value) if key in {"wall", "cpu"} else int(value)
                for line in log.read_text().splitlines():
                    if "maximum resident set size" in line:
                        row["rss_bytes"] = int(line.split()[0])
                records.write(json.dumps(row) + "\n")
                records.flush()
                print(json.dumps(row), flush=True)
with (output / "load.jsonl").open("w") as records:
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"]:
        for variant, (root, exe) in variants.items():
            run = subprocess.run(
                [str(exe), "load", str(root / f"{name}.hsd")],
                text=True,
                capture_output=True,
                check=True,
            )
            for line in run.stdout.splitlines():
                _, turn, *fields = line.split()
                row = {"dictionary": name, "variant": variant, "round": int(turn)}
                row.update({k: float(v) for k, v in (f.split("=") for f in fields)})
                records.write(json.dumps(row) + "\n")
