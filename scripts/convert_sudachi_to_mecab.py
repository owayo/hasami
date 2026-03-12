"""SudachiDict winfo ダンプを hasami (MeCab互換) CSV形式に変換する.

Usage:
    # 1. sudachi.rs で winfo, matrix, pos をダンプ
    sudachi dump system.dic winfo winfo.txt
    sudachi dump system.dic matrix matrix.def
    sudachi dump system.dic pos pos.txt

    # 2. このスクリプトで変換
    python scripts/convert_sudachi_to_mecab.py \
        --winfo winfo.txt \
        --matrix matrix.def \
        --output-dir sudachi-csv/

    # 3. hasami で辞書をビルド
    hasami build -i sudachi-csv/ -o dict/sudachi.hsd

winfo CSV format (19 fields):
    [0]  surface
    [1]  left_id
    [2]  right_id
    [3]  cost
    [4]  normalized_form (base_form)
    [5]  POS1
    [6]  POS2
    [7]  POS3
    [8]  POS4
    [9]  conjugation_type
    [10] conjugation_form
    [11] reading
    [12] surface_form (書字形出現形 — NOT pronunciation)
    [13] ?
    [14] split_mode (A/B/C)
    [15-18] other
"""

import argparse
import csv
import shutil
import sys
from pathlib import Path


def convert_winfo(winfo_path, output_dir):
    """winfo.txt を MeCab互換 CSV に変換.

    MeCab CSV format (13 fields):
        surface, left_id, right_id, cost,
        POS1, POS2, POS3, POS4,
        conjugation_type, conjugation_form,
        base_form, reading, pronunciation

    Returns:
        int: 変換されたエントリ数.

    """
    output_dir.mkdir(parents=True, exist_ok=True)
    output_csv = output_dir / "sudachi.csv"

    total = 0
    skipped = 0

    with (
        open(winfo_path, encoding="utf-8") as fin,
        open(output_csv, "w", encoding="utf-8", newline="") as fout,
    ):
        reader = csv.reader(fin)
        writer = csv.writer(fout)

        for row in reader:
            if len(row) < 13:
                skipped += 1
                continue

            surface = row[0]
            left_id = row[1]
            right_id = row[2]
            cost = row[3]

            # left_id=-1 のエントリはスキップ (仮想エントリ)
            if left_id == "-1" or right_id == "-1":
                skipped += 1
                continue

            # 空のsurface はスキップ
            if not surface.strip():
                skipped += 1
                continue

            base_form = row[4] if row[4] and row[4] != "*" else surface
            pos1 = row[5] if row[5] else "*"
            pos2 = row[6] if row[6] else "*"
            pos3 = row[7] if row[7] else "*"
            pos4 = row[8] if row[8] else "*"
            conj_type = row[9] if row[9] else "*"
            conj_form = row[10] if row[10] else "*"
            reading = row[11] if row[11] and row[11] != "*" else ""
            # SudachiDict の field 12 は書字形出現形（surface_form）であり、
            # 発音形ではない。発音形がないため reading をそのまま使う。
            # 助詞の読み替え（は→ワ等）は kotonoha の NJD 処理で行われる。
            pronunciation = reading

            # MeCab互換CSV: 13フィールド
            writer.writerow(
                [
                    surface,
                    left_id,
                    right_id,
                    cost,
                    pos1,
                    pos2,
                    pos3,
                    pos4,
                    conj_type,
                    conj_form,
                    base_form,
                    reading,
                    pronunciation,
                ]
            )
            total += 1

    print(f"Converted: {total:,} entries", file=sys.stderr)
    print(f"Skipped:   {skipped:,} entries", file=sys.stderr)
    print(f"Output:    {output_csv}", file=sys.stderr)
    return total


def copy_matrix(matrix_path, output_dir):
    """matrix.def をそのままコピー (形式はMeCabと同一).

    Returns:
        Path: コピー先パス.

    """
    dest = output_dir / "matrix.def"
    shutil.copy2(matrix_path, dest)

    with open(matrix_path, encoding="utf-8") as f:
        header = f.readline().strip().split()
        left = header[0]
        right = header[1]

    print(
        f"Matrix:    {left}x{right} -> {dest}",
        file=sys.stderr,
    )
    return dest


def main():
    """SudachiDict を hasami 形式に変換."""
    parser = argparse.ArgumentParser(
        description="SudachiDict winfo dump -> hasami MeCab CSV"
    )
    parser.add_argument(
        "--winfo",
        required=True,
        help="Path to winfo dump (from sudachi dump ... winfo)",
    )
    parser.add_argument(
        "--matrix",
        required=True,
        help="Path to matrix dump (from sudachi dump ... matrix)",
    )
    parser.add_argument(
        "--output-dir",
        required=True,
        help="Output directory for MeCab-format CSV + matrix.def",
    )
    args = parser.parse_args()

    output_dir = Path(args.output_dir)
    convert_winfo(args.winfo, output_dir)
    copy_matrix(args.matrix, output_dir)

    print(file=sys.stderr)
    print(
        f"Next step: hasami build -i {output_dir} -o dict/sudachi.hsd",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
