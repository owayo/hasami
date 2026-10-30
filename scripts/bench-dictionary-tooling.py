"""辞書を全件なめるコマンド（info --verify・export）を、変更前後の CLI を交互に実行して測る。

解析（`hasami bench`）と違い、`Dictionary::for_each_entry` と `Dictionary::verify` が通る
全群の列挙を測る。出力が変更前後で一致することも毎回確かめる。
"""

import argparse
import ctypes
import hashlib
import json
import os
import statistics
import subprocess
import tempfile
import time
from pathlib import Path

# 測る操作。{name: (引数, 出力をファイルに書くか)}
JOBS = {
    "verify": (("info", "--dict", "{dict}", "--verify"), False),
    "export": (("export", "--dict", "{dict}", "--output", "{out}"), True),
}


def process_metrics(process):
    """Windows の子プロセスの CPU 時間と最大 Working Set を取得する。

    Returns:
        dict: CPU 時間と最大 Working Set。Windows 以外では空の辞書。
    """
    if os.name != "nt":
        return {}

    class Counters(ctypes.Structure):
        _fields_ = [("cb", ctypes.c_ulong), ("faults", ctypes.c_ulong)] + [
            (name, ctypes.c_size_t)
            for name in (
                "peak_working_set",
                "working_set",
                "peak_paged",
                "paged",
                "peak_nonpaged",
                "nonpaged",
                "pagefile",
                "peak_pagefile",
            )
        ]

    counters = Counters()
    counters.cb = ctypes.sizeof(counters)
    handle = ctypes.c_void_p(int(process._handle))
    if not ctypes.windll.psapi.GetProcessMemoryInfo(
        handle, ctypes.byref(counters), counters.cb
    ):
        raise ctypes.WinError()
    times = [ctypes.c_ulonglong() for _ in range(4)]
    if not ctypes.windll.kernel32.GetProcessTimes(
        handle, *(ctypes.byref(value) for value in times)
    ):
        raise ctypes.WinError()
    return {
        "cpu_ms": (times[2].value + times[3].value) / 10_000,
        "peak_working_set_bytes": counters.peak_working_set,
    }


def digest_of(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def run(binary, job, dictionary, workdir):
    """1 回分を測り、(行, 出力の SHA-256) を返す。"""
    template, writes = JOBS[job]
    out = workdir / f"{job}.out"
    command = [
        str(binary),
        *(part.format(dict=str(dictionary), out=str(out)) for part in template),
    ]
    start = time.perf_counter_ns()
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    stdout, stderr = process.communicate()
    elapsed = (time.perf_counter_ns() - start) / 1_000_000
    if process.returncode:
        raise RuntimeError((stdout + stderr).decode(errors="replace"))
    row = {"wall_ms": elapsed, **process_metrics(process)}
    if writes:
        return row, digest_of(out)
    # verify はロードと検証の所要時間が実行ごとに変わるので、その行を除いて比べる。
    report = [
        line
        for line in stdout.decode("utf-8", errors="replace").splitlines()
        if not line.startswith(("Load time:", "Verify:"))
    ]
    return row, hashlib.sha256("\n".join(report).encode("utf-8")).hexdigest()


def summarize(values):
    ordered = sorted(values)
    return {
        "median": statistics.median(ordered),
        "min": min(ordered),
        "max": max(ordered),
        "samples": ordered,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--dict", type=Path, action="append", required=True)
    parser.add_argument("--job", action="append", choices=sorted(JOBS))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=5)
    args = parser.parse_args()
    if args.rounds < 2:
        parser.error("--rounds は 2 以上を指定する")

    binaries = {"before": args.before.resolve(), "after": args.after.resolve()}
    jobs = args.job or sorted(JOBS)
    rows = []
    with tempfile.TemporaryDirectory() as temp:
        workdir = Path(temp)
        for dictionary in args.dict:
            dictionary = dictionary.resolve()
            for job in jobs:
                digests = {}
                # turn -1 はウォームアップ（ページキャッシュをそろえる）。
                for turn in range(-1, args.rounds):
                    # 先に走るバイナリを回ごとに交替し、負荷の偏りを両方に散らす。
                    order = (
                        ("before", "after") if turn % 2 == 0 else ("after", "before")
                    )
                    for variant in order:
                        row, digest = run(binaries[variant], job, dictionary, workdir)
                        digests.setdefault(variant, digest)
                        if digests[variant] != digest:
                            raise RuntimeError(
                                f"{job} の出力が {variant} の実行間で変わった"
                            )
                        if turn < 0:
                            continue
                        rows.append(
                            {
                                "dictionary": dictionary.name,
                                "job": job,
                                "round": turn,
                                "variant": variant,
                                **row,
                            }
                        )
                        print(json.dumps(rows[-1]), flush=True)
                if digests["before"] != digests["after"]:
                    raise RuntimeError(
                        f"{dictionary.name} の {job} の出力が変更前後で違う"
                    )

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        "".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8", newline="\n"
    )
    summary = {}
    for dictionary in dict.fromkeys(row["dictionary"] for row in rows):
        for job in jobs:
            key = f"{dictionary}/{job}"
            summary[key] = {}
            for variant in binaries:
                picked = [
                    row
                    for row in rows
                    if row["dictionary"] == dictionary
                    and row["job"] == job
                    and row["variant"] == variant
                ]
                summary[key][variant] = {
                    metric: summarize([row[metric] for row in picked])
                    for metric in ("wall_ms", "cpu_ms", "peak_working_set_bytes")
                    if metric in picked[0]
                }
            for metric in summary[key]["before"]:
                before = summary[key]["before"][metric]["median"]
                after = summary[key]["after"][metric]["median"]
                summary[key].setdefault("change_percent", {})[metric] = (
                    (after - before) / before * 100 if before else None
                )
            print(f"{key}: {json.dumps(summary[key].get('change_percent'))}")
    args.output.with_name("summary.json").write_text(
        json.dumps(summary, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )


if __name__ == "__main__":
    main()
