#!/usr/bin/env python3
"""Convert UniDic CSV files to IPAdic-compatible field order for hasami.

UniDic CSV field layout (14+ fields):
  0: surface, 1: left_id, 2: right_id, 3: cost,
  4-7: pos1-pos4, 8: cType, 9: cForm,
  10: lForm (語彙素読み), 11: lemma (語彙素),
  12: orth (書字形出現形), 13: pron (発音形出現形), ...

IPAdic CSV field layout (13 fields):
  0: surface, 1: left_id, 2: right_id, 3: cost,
  4-7: pos1-pos4, 8: cType, 9: cForm,
  10: base_form, 11: reading, 12: pronunciation

Mapping:
  base_form    <- lemma (field 11)
  reading      <- lForm (field 10)
  pronunciation <- pron  (field 13)
"""

import csv
import glob
import os
import sys


def convert_file(input_path: str, output_path: str) -> int:
    """Convert a single UniDic CSV file to IPAdic-compatible format.

    Returns:
        Number of entries converted.

    """
    count = 0
    with (
        open(input_path, encoding="utf-8") as inf,
        open(output_path, "w", encoding="utf-8", newline="") as outf,
    ):
        reader = csv.reader(inf)
        writer = csv.writer(outf)
        for row in reader:
            if len(row) < 14:
                writer.writerow(row)
                count += 1
                continue
            out_row = [
                row[0],  # surface
                row[1],  # left_id
                row[2],  # right_id
                row[3],  # cost
                row[4],  # pos1
                row[5],  # pos2
                row[6],  # pos3
                row[7],  # pos4
                row[8],  # cType
                row[9],  # cForm
                row[11],  # base_form  <- lemma
                row[10],  # reading    <- lForm
                row[13],  # pronunciation <- pron
            ]
            writer.writerow(out_row)
            count += 1
    return count


def main() -> None:
    """Convert all UniDic CSV files in a directory to IPAdic-compatible format."""
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <input-dir> <output-dir>", file=sys.stderr)
        sys.exit(1)

    input_dir = sys.argv[1]
    output_dir = sys.argv[2]
    os.makedirs(output_dir, exist_ok=True)

    csv_files = sorted(glob.glob(os.path.join(input_dir, "*.csv")))
    if not csv_files:
        print(f"Error: No CSV files found in {input_dir}", file=sys.stderr)
        sys.exit(1)

    total = 0
    for csv_file in csv_files:
        basename = os.path.basename(csv_file)
        output_path = os.path.join(output_dir, basename)
        count = convert_file(csv_file, output_path)
        total += count
        print(f"  {basename}: {count} entries")

    print(f"Converted {len(csv_files)} files ({total} entries) -> {output_dir}")


if __name__ == "__main__":
    main()
