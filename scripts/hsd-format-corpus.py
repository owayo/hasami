"""Build the 2026-09-26 corpus: ARCHIVE BASE_CHECKOUT OUTPUT_DIRECTORY.

BASE_CHECKOUT is 23e5e51a87a9ababfa3f326f72fd67ca79abe1cc.
ARCHIVE is ldcc-20140209.tar.gz; no extracted articles are written to disk.
"""

import hashlib
import itertools
import random
import sys
import tarfile
from pathlib import Path

archive_path, base, out = map(Path, sys.argv[1:])
out.mkdir(parents=True, exist_ok=True)
with tarfile.open(archive_path) as archive:
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
    news = [
        line
        for m in articles
        for line in archive.extractfile(m).read().decode("utf-8-sig").splitlines()[3:]
        if line.strip()
    ]
tech = [
    line
    for p in [base / "README.md", *sorted((base / "docs").glob("*.md"))]
    for line in p.read_text().splitlines()
    if line.strip()
]
edge = [
    "".join(parts)
    for parts in itertools.product(
        [
            "",
            "さて、",
            "「",
            "昔、",
            "どうか",
            "および",
            "一方で、",
            "こんにちは。",
            "　",
        ],
        [
            "私",
            "彼女",
            "東京の人々",
            "金",
            "何",
            "一人の旅人",
            "𠮷野家",
            "Zoom",
            "計算の結果",
        ],
        [
            "がない",
            "ではありません",
            "を読み込んだ",
            "を走らせよう",
            "が歩いていた",
            "の方法を示した",
            "に住んでいる",
            "と話していた",
        ],
        [
            "。",
            "？",
            "！",
            "……。",
            "」と答えた。",
            " 50%で動く。",
            "\tCSVで出力する。",
            "——。",
        ],
    )
]
rng = random.Random(20260926)
alphabet = "あいうえおがないんカタカナ・ー漢字東京一二三〇９１ＡＢ012aZ%℃‰（）「」!?。．…—\t 𠮷😀\u200d\ufe0f\u0301"
edge.extend(
    "".join(rng.choice(alphabet) for _ in range(rng.randint(1, 120)))
    for _ in range(4000)
)
edge.extend(f"microSD×C {i}℃ K−POP が好きだ。" for i in range(100))
edge.extend(
    ["x" * 10000, "カタカナ" * 1000, "あ" * 10000, "。" * 10000, "\x00制御文字\x01\x02"]
)
for name, lines in [
    ("news", news),
    ("tech", tech),
    ("edge", edge),
    ("mixed", news + tech + edge),
]:
    data = ("\n".join(lines) + "\n").encode()
    (out / f"{name}.txt").write_bytes(data)
    print(
        f"{name} lines={len(lines)} bytes={len(data)} sha256={hashlib.sha256(data).hexdigest()}"
    )
