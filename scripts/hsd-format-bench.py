"""Alternate three format readers; run after other builds/benchmarks finish.

Usage: python hsd-format-bench.py SCRATCH OLD_DICT_DIR [ROUNDS]
SCRATCH holds eval-v4, eval-v5, eval-fixed, varint/, fixed/, and mixed.txt.
Writes bench.jsonl and separate macOS time logs (including maximum RSS).
"""

import json
import subprocess
import sys
from pathlib import Path

root = Path(sys.argv[1]).resolve()
old = Path(sys.argv[2]).resolve()
rounds = int(sys.argv[3]) if len(sys.argv) > 3 else 6
variants = ["v4", "v5", "fixed"]
dirs = {"v4": old, "v5": root / "varint", "fixed": root / "fixed"}
short = root / "short.txt"
short.write_text("東京都に住んでいる人々が増えている。\n" * 100_000)
with (root / "bench.jsonl").open("w") as output:
    for corpus in ["mixed", "short"]:
        for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"]:
            for turn in range(rounds):
                order = variants[turn % 3 :] + variants[: turn % 3]
                if turn % 2:
                    order = order[::-1]
                for variant in order:
                    log = root / f"time-{corpus}-{name}-{turn}-{variant}.txt"
                    command = [
                        "/usr/bin/time",
                        "-l",
                        str(root / f"eval-{variant}"),
                        "bench",
                        str(dirs[variant] / f"{name}.hsd"),
                        str(root / f"{corpus}.txt"),
                        "1",
                    ]
                    with log.open("w") as err:
                        run = subprocess.run(
                            command,
                            text=True,
                            capture_output=False,
                            stdout=subprocess.PIPE,
                            stderr=err,
                            check=True,
                        )
                    row = {
                        "corpus": corpus,
                        "dictionary": name,
                        "round": turn,
                        "variant": variant,
                    }
                    row.update(
                        {
                            k: float(v)
                            for k, v in (
                                field.split("=") for field in run.stdout.split()
                            )
                        }
                    )
                    output.write(json.dumps(row) + "\n")
                    output.flush()
                    print(json.dumps(row), flush=True)
