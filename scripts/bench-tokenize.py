r"""hasami の解析の費用を、版どうし（変更前後）の CLI を交互に走らせて比べる。

Issue #119 と同じ固定の入力（20 種の文を数値を変えて並べた 6,000 行。
約半分が数量の表記を含む）を `--repeat` 回並べ（`--input` で別のコーパスも渡せる）、
各 CLI で次の 2 つを交互に測る。`--case` で、補正の錨の文字を含まない文だけ（plain）、
錨はあるが補正の当たらない文（idle）、単位の多い長い 1 行（units）にも替えられる。

- `tokenize -f wakachi -j 1`
  （標準入力から全行。辞書のロードと出力を含むプロセス全体）
- `bench --file`（解析だけ。3 回の最速の回 Best pass）

プロセス全体の指標は OS ごとに取れるものを記録する。

- macOS: `/usr/bin/time -l` の instructions retired・cycles elapsed・最大 RSS
- Linux: `perf stat -e instructions:u,cycles:u`
  （perf が無い・権限が無いときは欠測）
- Windows: `QueryProcessCycleTime` のサイクル（命令数ではない）、
  `GetProcessTimes` の CPU 時間、最大 Working Set。
  論理 CPU 1 個（`--cpu`）に固定して走らせる

分かち書きの出力（版どうしで区切りが同じか）も毎回確かめ、
全標本と SHA-256 を JSON に残す。辞書の形式は版で変わるので、
`名前=CLI=辞書` の組で渡す（最初の組を基準にした比を出す）。

    python scripts/bench-tokenize.py --output bench-out \
        --variant before=old/hasami=old/ipadic.hsd \
        --variant after=target/release/hasami=dict/ipadic.hsd
"""

import argparse
import contextlib
import ctypes
import hashlib
import json
import os
import platform
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

# Issue #119 の入力。数値の入る文は {n} と {m} を行ごとに変える
SENTENCES = [
    "朝の電車は思ったより空いていて、窓の外の川がよく見えた。",
    "会議の資料は前の晩に作り直したが、結局ほとんど使わなかった。",
    "駅前のパン屋は火曜日が休みなので、帰りに隣の店で牛乳だけ買った。",
    "新しい設定では、保存の間隔を{n}分に短くし、古い記録は{m}日で消す。",
    "このコマンドは引数を{n}個まで受け付け、それより多いと誤りを返す。",
    "庭の柿は今年{n}個ほど実をつけたが、半分は鳥に食べられた。",
    "子どもの頃に通った図書館は、建て替えで場所が少し北へ移った。",
    "体重が{n}.{m}kg減ったと聞いて、母は電話の向こうで笑っていた。",
    "週末の山道は落ち葉で滑りやすく、下りに思ったより時間がかかった。",
    "見積もりは{n}万円で、納期は来月の{m}日と書いてあった。",
    "手紙を書くのは久しぶりで、宛名の字が何度も曲がった。",
    "古いノートの間から、{n}年前の映画の半券が出てきた。",
    "台所の蛇口から水が漏れるので、週明けに業者を呼ぶことにした。",
    "測定は{n}回繰り返し、ばらつきが{m}%を超えた回は除いた。",
    "夜になると、遠くの工場の明かりが規則正しく並んで見える。",
    "隣の部屋から聞こえるピアノは、毎日同じ曲の同じ所で止まる。",
    "表の三列目には、地域ごとの人口と、前の年からの増減を入れる。",
    "停電は{n}時間ほどで戻ったが、冷蔵庫の氷はすっかり溶けていた。",
    "雨の日は洗濯物が乾かないので、浴室の換気扇を一晩中回しておく。",
    "第{n}章では、読み手が最初につまずく所を例で説明している。",
]


# 入力の種類（SENTENCES の番号）
# - issue: Issue と同じ 20 種
# - plain: 補正の錨の文字（数字・漢数字・月・年・日・成句の先頭字など。
#   src/anchors.rs）を含まない文
# - idle: 錨の文字（日・間）はあるが、補正の当たらない文
# - units: 単位付きの数量の文を 1 行に 200 文ずつつないだ長い入力
#   （文書を 1 回で解析する経路）
CASES = {
    "issue": range(len(SENTENCES)),
    "plain": [0, 1, 6, 10, 12, 14],
    "idle": [2, 8, 15],
}
UNIT_SENTENCE = "体重は{n}.{m}kgで、ばらつきは{m}%、距離は{n}kmだった。"


def case_text(case, repeat):
    """入力（6,000 文を repeat 回）を返す。

    issue の 1 回分は Issue と同じ 525,601 バイト。
    """
    if case == "units":
        sentences = [
            UNIT_SENTENCE.format(n=(i * 7) % 97 + 1, m=(i * 13) % 31 + 1)
            for i in range(6000)
        ]
        lines = ["".join(sentences[i : i + 200]) for i in range(0, 6000, 200)]
    else:
        picked = [SENTENCES[i] for i in CASES[case]]
        lines = [
            picked[i % len(picked)].format(n=(i * 7) % 97 + 1, m=(i * 13) % 31 + 1)
            for i in range(6000)
        ]
    return ("\n".join(lines) + "\n") * repeat


def digest(path):
    """ファイルの SHA-256。"""
    with open(path, "rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def windows_metrics(process):
    """Windows の子プロセスのサイクル・CPU 時間・最大 Working Set。"""

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

    handle = ctypes.c_void_p(int(process._handle))
    cycles = ctypes.c_ulonglong()
    if not ctypes.windll.kernel32.QueryProcessCycleTime(handle, ctypes.byref(cycles)):
        raise ctypes.WinError()
    times = [ctypes.c_ulonglong() for _ in range(4)]
    if not ctypes.windll.kernel32.GetProcessTimes(
        handle, *(ctypes.byref(value) for value in times)
    ):
        raise ctypes.WinError()
    counters = Counters()
    counters.cb = ctypes.sizeof(counters)
    if not ctypes.windll.psapi.GetProcessMemoryInfo(
        handle, ctypes.byref(counters), counters.cb
    ):
        raise ctypes.WinError()
    return {
        "cycles": cycles.value,
        "cpu_s": (times[2].value + times[3].value) / 1e7,
        "peak_working_set_bytes": counters.peak_working_set,
    }


def counter_command(command):
    """OS の計数器を付けたコマンドと、その出力の読み方を返す。"""
    system = platform.system()
    if system == "Darwin" and Path("/usr/bin/time").exists():
        return ["/usr/bin/time", "-l", *command], parse_bsd_time
    if system == "Linux" and shutil.which("perf"):
        return [
            "perf",
            "stat",
            "-x",
            ",",
            "-e",
            "instructions:u,cycles:u",
            "--",
            *command,
        ], parse_perf_stat
    return command, None


def parse_bsd_time(stderr):
    """macOS の /usr/bin/time -l の出力から命令数・サイクル・最大 RSS を取る。"""
    metrics = {}
    for key, pattern in (
        ("instructions", r"(\d+)\s+instructions retired"),
        ("cycles", r"(\d+)\s+cycles elapsed"),
        ("max_rss_bytes", r"(\d+)\s+maximum resident set size"),
    ):
        match = re.search(pattern, stderr)
        if match:
            metrics[key] = int(match.group(1))
    return metrics


def parse_perf_stat(stderr):
    """perf stat -x , の出力から命令数・サイクルを取る（数えられなければ欠測）。"""
    metrics = {}
    for line in stderr.splitlines():
        fields = line.split(",")
        if len(fields) >= 3 and fields[0].isdigit():
            if fields[2].startswith("instructions"):
                metrics["instructions"] = int(fields[0])
            elif fields[2].startswith("cycles"):
                metrics["cycles"] = int(fields[0])
    return metrics


def run(command, stdin_path, stdout_path):
    """1 回走らせ、指標を返す（標準出力は stdout_path に書く）。"""
    wrapped, parse = counter_command(command)
    with contextlib.ExitStack() as stack:
        stdin = (
            stack.enter_context(open(stdin_path, "rb"))
            if stdin_path
            else subprocess.DEVNULL
        )
        stdout = stack.enter_context(open(stdout_path, "wb"))
        start = time.perf_counter()
        process = subprocess.Popen(
            wrapped, stdin=stdin, stdout=stdout, stderr=subprocess.PIPE
        )
        _, stderr = process.communicate()
        wall = time.perf_counter() - start
    stderr = stderr.decode("utf-8", errors="replace")
    if process.returncode:
        raise RuntimeError(
            f"{' '.join(command)} が {process.returncode} で終わった: {stderr}"
        )
    row = {"wall_s": wall}
    if os.name == "nt":
        row.update(windows_metrics(process))
    elif parse:
        row.update(parse(stderr))
    return row


def summarize(rows, base_rows):
    """指標ごとの最小値・中央値と、基準の版との比。"""
    summary = {}
    for key in sorted({k for row in rows for k in row}):
        values = [row[key] for row in rows if key in row]
        base = [row[key] for row in base_rows if key in row]
        if (
            not values
            or not base
            or not all(isinstance(v, (int, float)) for v in values)
        ):
            continue
        summary[key] = {
            "min": min(values),
            "median": statistics.median(values),
            "ratio_min": min(values) / min(base) - 1,
            "ratio_median": statistics.median(values) / statistics.median(base) - 1,
            "samples": values,
        }
    return summary


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--variant",
        action="append",
        required=True,
        metavar="NAME=CLI=DICT",
        help="比べる版（2 つ以上。最初の版を基準にする）",
    )
    parser.add_argument("--output", type=Path, required=True, help="結果の置き場所")
    parser.add_argument("--rounds", type=int, default=6, help="交互に走らせる回数")
    parser.add_argument(
        "--case",
        choices=["issue", "plain", "idle", "units"],
        default="issue",
        help="固定の入力の種類（既定は Issue と同じ issue）",
    )
    parser.add_argument("--repeat", type=int, default=20, help="固定の入力を並べる回数")
    parser.add_argument(
        "--input", type=Path, help="固定の入力の代わりに使う 1 行 1 文のファイル"
    )
    parser.add_argument(
        "--cpu", type=int, default=2, help="固定する論理 CPU の番号（Windows・Linux）"
    )
    parser.add_argument(
        "--job",
        action="append",
        choices=["wakachi", "bench"],
        help="測る操作（既定は両方）",
    )
    args = parser.parse_args()
    variants = []
    for spec in args.variant:
        name, binary, dictionary = spec.split("=", 2)
        variants.append((name, Path(binary).resolve(), Path(dictionary).resolve()))
    if len(variants) < 2 or args.rounds < 2:
        parser.error("--variant は 2 つ以上、--rounds は 2 以上を指定する")
    jobs = args.job or ["wakachi", "bench"]

    # 子プロセスは親の CPU の割り当てを引き継ぐ
    if os.name == "nt":
        kernel32 = ctypes.windll.kernel32
        kernel32.GetCurrentProcess.restype = ctypes.c_void_p
        kernel32.SetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
        if not kernel32.SetProcessAffinityMask(
            kernel32.GetCurrentProcess(), 1 << args.cpu
        ):
            raise ctypes.WinError()
    elif hasattr(os, "sched_setaffinity"):
        os.sched_setaffinity(0, {args.cpu})

    args.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as work:
        work = Path(work)
        if args.input:
            input_path = args.input.resolve()
        else:
            input_path = work / "input.txt"
            input_path.write_bytes(case_text(args.case, args.repeat).encode("utf-8"))
        manifest = {
            "input": {
                "path": str(input_path) if args.input else None,
                "case": None if args.input else args.case,
                "repeat": None if args.input else args.repeat,
                "bytes": input_path.stat().st_size,
                "sha256": digest(input_path),
            },
            "platform": platform.platform(),
            "processor": platform.processor(),
            "rounds": args.rounds,
            "variants": [],
        }
        for name, binary, dictionary in variants:
            version = subprocess.run(
                [binary, "--version"], capture_output=True, text=True, check=True
            ).stdout.strip()
            manifest["variants"].append(
                {
                    "name": name,
                    "version": version,
                    "binary_sha256": digest(binary),
                    "dict_sha256": digest(dictionary),
                }
            )

        def command(job, binary, dictionary):
            if job == "wakachi":
                return [
                    binary,
                    "tokenize",
                    "-d",
                    dictionary,
                    "-f",
                    "wakachi",
                    "-j",
                    "1",
                ], True
            return [
                binary,
                "bench",
                "--dict",
                dictionary,
                "--file",
                input_path,
                "-i",
                "3",
            ], False

        samples = {job: {name: [] for name, _, _ in variants} for job in jobs}
        outputs = {name: set() for name, _, _ in variants}
        # 1 回目はページキャッシュを温めるだけで捨てる。以後は版の順を毎回反転する
        for round_index in range(args.rounds + 1):
            order = variants if round_index % 2 == 0 else list(reversed(variants))
            for job in jobs:
                for name, binary, dictionary in order:
                    cmd, uses_stdin = command(job, binary, dictionary)
                    out = work / f"{name}-{job}.out"
                    row = run(
                        [str(c) for c in cmd], input_path if uses_stdin else None, out
                    )
                    if job == "wakachi":
                        outputs[name].add(digest(out))
                    else:
                        best = re.search(
                            r"Best pass: ([0-9.]+)s", out.read_text("utf-8")
                        )
                        row["best_pass_s"] = float(best.group(1)) if best else None
                    if round_index:
                        samples[job][name].append(row)

    base_name = variants[0][0]
    report = {"manifest": manifest, "samples": samples, "summary": {}}
    for job in jobs:
        report["summary"][job] = {
            name: summarize(samples[job][name], samples[job][base_name])
            for name, _, _ in variants
        }
    same = len(set().union(*outputs.values())) == 1 and all(
        len(v) == 1 for v in outputs.values()
    )
    report["wakachi_outputs"] = {name: sorted(v) for name, v in outputs.items()}
    report["wakachi_same"] = same
    (args.output / "bench-tokenize.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=1, default=str), encoding="utf-8"
    )

    for job in jobs:
        print(f"== {job}（最小値 / 中央値、{base_name} 比）")
        for name, _, _ in variants:
            summary = report["summary"][job][name]
            parts = []
            for key in ("instructions", "cycles", "best_pass_s", "cpu_s", "wall_s"):
                if key in summary:
                    s = summary[key]
                    parts.append(
                        f"{key} {s['min']:.4g} / {s['median']:.4g} "
                        f"({s['ratio_min']:+.1%} / {s['ratio_median']:+.1%})"
                    )
            print(f"  {name}: " + "; ".join(parts))
    if "wakachi" in jobs:
        if same:
            print("分かち書きの出力: すべて同じ")
        else:
            # 同じ出力の版をまとめて示す（回によって出力が変わった版は、ひとまとまりにしない）
            groups = {}
            for name, digests in outputs.items():
                key = tuple(sorted(digests)) if len(digests) == 1 else (name,)
                groups.setdefault(key, []).append(name)
            print(
                "分かち書きの出力: 版によって違う（同じ出力の組: "
                + " / ".join(" = ".join(names) for names in groups.values())
                + "）"
            )
    sys.stdout.flush()


if __name__ == "__main__":
    main()
