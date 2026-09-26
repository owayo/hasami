"""Measure the current release binary on three local dictionaries (macOS only).

Run from the repository root with a scratch directory containing
ldcc-20140209.tar.gz as the sole argument. Results go to that directory.
"""

import hashlib
import json
import subprocess
import sys
import tarfile
from pathlib import Path

root = Path(sys.argv[1])
with tarfile.open(root / "ldcc-20140209.tar.gz") as archive:
    articles = sorted(
        (
            m
            for m in archive.getmembers()
            if m.isfile()
            and m.name.endswith(".txt")
            and "LICENSE" not in m.name
            and "README" not in m.name
        ),
        key=lambda m: m.name,
    )
    lines = []
    for member in articles:
        text = archive.extractfile(member).read().decode("utf-8-sig")
        lines.extend(line for line in text.splitlines()[3:] if line.strip())
corpus = root / "news.txt"
corpus.write_text("\n".join(lines) + "\n")
with (root / "baseline.txt").open("w") as out:
    out.write(
        f"corpus_lines={len(lines)} bytes={corpus.stat().st_size} "
        f"sha256={hashlib.sha256(corpus.read_bytes()).hexdigest()}\n"
    )
    out.flush()
    for name in ["ipadic", "ipadic-neologd", "ipadic-neologd-sudachi"]:
        d = Path("dict") / (name + ".hsd")
        out.write(
            f"\nDICT {name} sha256={hashlib.sha256(d.read_bytes()).hexdigest()}\n"
        )
        out.flush()
        for args in [
            ["info", "--verify"],
            ["bench", "--iterations", "20000"],
            [
                "bench",
                "--iterations",
                "5000",
                "--text",
                "形態素解析エンジンは辞書をメモリに配置し、入力された文章の候補を調べて最適な経路を選びます。格納するデータが小さくなっても、読み出しに余計な処理が必要なら、解析全体が速くなるとは限りません。",
            ],
            ["bench", "--file", str(corpus), "--iterations", "3"],
            ["tokenize", "東京都に住んでいる人々が増えている。"],
        ]:
            cmd = [
                "/usr/bin/time",
                "-l",
                "target/release/hasami",
                *args,
                "--dict",
                str(d),
            ]
            out.write(json.dumps(cmd, ensure_ascii=False) + "\n")
            out.flush()
            subprocess.run(cmd, stdout=out, stderr=out, check=True)
            out.flush()
