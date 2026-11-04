#!/usr/bin/env python3
"""既存のユーザー辞書から、明示された語だけを一時CSVへ選び出す。"""

import argparse
import csv
import re
import sys
from pathlib import Path


def validate_row(row: tuple[str, ...]) -> None:
    """選択した行が13列のMeCab形式であることを確認する。

    Raises:
        ValueError: 列数、文脈ID、コストが不正な場合。

    """
    if len(row) != 13:
        raise ValueError("選択した辞書エントリが13列ではありません")
    for index, minimum, maximum in [(1, 0, 65535), (2, 0, 65535), (3, -32768, 32767)]:
        pattern = r"[+]?[0-9]+" if index < 3 else r"[+-]?[0-9]+"
        if (
            re.fullmatch(pattern, row[index]) is None
            or not minimum <= int(row[index]) <= maximum
        ):
            raise ValueError("選択した辞書エントリの文脈IDまたはコストが不正です")


def select_words(input_directory: Path, words: list[str]) -> list[tuple[str, ...]]:
    """語の別読みと素性を保ち、完全に同じ行だけを重複排除する。

    Returns:
        ファイル名順・元の行順に並べた、選択対象の全エントリ。

    Raises:
        ValueError: 要求した語が不足するか入力CSVが不正な場合。

    """
    required = set(words)
    if not required or "" in required:
        raise ValueError("選択する語を空にできません")
    inputs = sorted(input_directory.glob("*.csv"))
    if not inputs:
        raise ValueError("入力CSVがありません")
    rows = []
    seen = set()
    found = set()
    for path in inputs:
        try:
            with path.open(encoding="utf-8-sig", newline="") as stream:
                for row in csv.reader(stream, strict=True):
                    if not row or row[0] not in required:
                        continue
                    entry = tuple(row)
                    validate_row(entry)
                    found.add(entry[0])
                    if entry not in seen:
                        seen.add(entry)
                        rows.append(entry)
        except (csv.Error, UnicodeError) as error:
            # 元のCSV値を診断やビルドログへ出さない。
            raise ValueError("入力CSVの構文または文字コードが不正です") from error
    if found != required:
        raise ValueError("選択を要求した語の登録が不足しています")
    return rows


def write_selection(input_directory: Path, words: list[str], output: Path) -> None:
    """正本を変更せず、検証済みの選択結果だけを別の場所へ書く。

    Raises:
        ValueError: 正本の下への出力を要求した場合や入力が不正な場合。

    """
    if output.resolve().is_relative_to(input_directory.resolve()):
        raise ValueError("出力先は入力ディレクトリの外にしてください")
    rows = select_words(input_directory, words)
    with output.open("w", encoding="utf-8", newline="") as stream:
        csv.writer(stream, lineterminator="\n").writerows(rows)


def main() -> int:
    """CLIを実行し、入力が不足・不正なら非ゼロで終了する。

    Returns:
        正常終了なら0、入出力または選択の失敗なら1。

    """
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input-dir", type=Path, required=True)
    parser.add_argument("--word", action="append", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        write_selection(args.input_dir, args.word, args.output)
    except (OSError, ValueError) as error:
        print(f"dictionary word selection: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
