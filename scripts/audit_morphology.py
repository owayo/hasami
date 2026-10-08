"""1 行 1 文のコーパスで、未知語・品詞・読み・UTF-8 位置と変更前後を調べる。

本文やトークンの全文は保存しない。ランキングには表層形が含まれるため、
私有のコーパスの出力は target/ 等に置く。正答率を測るスクリプトではない。
単語コストを含む全フィールドの一致確認には hsd-format-compare.py を使う。
"""

import argparse
import collections
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path


def analyze(binary, dictionary, corpus, output):
    """全行の解析を一時ファイルに書き、パイプの詰まりと大量のメモリ確保を避ける。"""
    with corpus.open("rb") as source:
        subprocess.run(
            [
                str(binary),
                "tokenize",
                "--dict",
                str(dictionary),
                "-f",
                "json",
                "-j",
                "1",
            ],
            stdin=source,
            stdout=output,
            stderr=subprocess.PIPE,
            check=True,
        )
    output.seek(0)


def count_tokens(text, tokens, counts, unknown, proper):
    """各トークンの位置・表層を検査し、監査の指標を数える。

    Raises:
        ValueError: 範囲が不正か、表層形が元の入力に一致しない場合。

    """
    data = text.encode("utf-8")
    previous_end = 0
    counts["lines"] += 1
    for token in tokens:
        start, end = token["start"], token["end"]
        if not previous_end <= start < end <= len(data):
            raise ValueError(f"invalid token range: {token}")
        if data[start:end].decode("utf-8") != token["surface"]:
            raise ValueError(f"surface differs from input: {token}")
        previous_end = end
        counts["tokens"] += 1
        key = (token["surface"], token["pos"], token["reading"])
        if not token["is_known"]:
            counts["unknown_tokens"] += 1
            unknown[key] += 1
        if token["pos"].startswith("名詞,固有名詞"):
            counts["proper_noun_tokens"] += 1
            proper[key] += 1
        if (
            any(c.isalpha() for c in token["surface"])
            and not token["reading"]
            and not token["pronunciation"]
            and not token["pos"].startswith("記号")
        ):
            counts["missing_readings"] += 1


def ranked(counter, limit):
    """同じ表層・品詞・読みの候補を頻度順に並べる。

    Returns:
        list: 頻度順の候補 (表層・品詞・読み・件数)。

    """
    return [
        {"surface": key[0], "pos": key[1], "reading": key[2], "count": count}
        for key, count in counter.most_common(limit)
    ]


def main():
    """コーパスの解析と比較を実行し、本文を含まない集計を保存する。

    Raises:
        ValueError: 解析結果の行数・位置・表層形が入力と整合しない場合。

    """
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--dict", type=Path, required=True)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--baseline-binary", type=Path)
    parser.add_argument("--baseline-dict", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=100)
    args = parser.parse_args()
    if bool(args.baseline_binary) != bool(args.baseline_dict):
        parser.error("baseline-binary and baseline-dict must be supplied together")
    if args.limit < 0:
        parser.error("limit must be nonnegative")

    keys = (
        "lines",
        "tokens",
        "unknown_tokens",
        "proper_noun_tokens",
        "missing_readings",
    )
    counts = collections.Counter(dict.fromkeys(keys, 0))
    before_counts = collections.Counter(dict.fromkeys(keys, 0))
    unknown, proper = collections.Counter(), collections.Counter()
    before_unknown, before_proper = collections.Counter(), collections.Counter()
    changes = collections.Counter()
    with tempfile.TemporaryFile() as after, tempfile.TemporaryFile() as before:
        analyze(args.binary.resolve(), args.dict.resolve(), args.corpus, after)
        if args.baseline_binary:
            analyze(
                args.baseline_binary.resolve(),
                args.baseline_dict.resolve(),
                args.corpus,
                before,
            )
        with args.corpus.open(encoding="utf-8") as source:
            for line in source:
                text = line.strip()
                if not text:
                    continue
                tokens = json.loads(after.readline())
                count_tokens(text, tokens, counts, unknown, proper)
                if not args.baseline_binary:
                    continue
                previous = json.loads(before.readline())
                count_tokens(
                    text, previous, before_counts, before_unknown, before_proper
                )
                if previous != tokens:
                    changes["changed_lines"] += 1
                previous_spans = [(t["start"], t["end"]) for t in previous]
                if previous_spans != [(t["start"], t["end"]) for t in tokens]:
                    changes["boundary_changed_lines"] += 1
        if after.readline() or before.readline():
            raise ValueError("analyzer returned more lines than the corpus")
    with args.corpus.open("rb") as source:
        corpus_hash = hashlib.file_digest(source, "sha256").hexdigest()
    report = {
        "corpus_sha256": corpus_hash,
        "after": dict(counts),
        "unknown": ranked(unknown, args.limit),
        "proper_nouns": ranked(proper, args.limit),
    }
    if args.baseline_binary:
        report.update(before=dict(before_counts), changes=dict(changes))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(
        json.dumps(
            {
                key: value
                for key, value in report.items()
                if key not in {"unknown", "proper_nouns"}
            }
        )
    )


if __name__ == "__main__":
    main()
