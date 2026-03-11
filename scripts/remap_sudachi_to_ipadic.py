"""SudachiDict CSV のPOS体系をIPAdic互換に変換し、left_id/right_idをリマップする.

SudachiDictの品詞体系(UniDic系)をIPAdicの品詞体系に変換し、
IPAdicのleft_id/right_idを割り当てることで、IPAdic matrixでの接続計算を可能にする。
"""

import argparse
import csv
import sys
from pathlib import Path

# SudachiDict POS -> IPAdic POS + (left_id, right_id) マッピング
# IPAdicのIDはIPAdic辞書から抽出した代表値
SUDACHI_TO_IPADIC = {
    # 名詞系
    ("名詞", "普通名詞", "一般"): ("名詞,一般,*,*", 1285),
    ("名詞", "普通名詞", "サ変可能"): ("名詞,サ変接続,*,*", 1283),
    ("名詞", "普通名詞", "形状詞可能"): ("名詞,形容動詞語幹,*,*", 1287),
    ("名詞", "普通名詞", "サ変形状詞可能"): ("名詞,サ変接続,*,*", 1283),
    ("名詞", "普通名詞", "副詞可能"): ("名詞,副詞可能,*,*", 1314),
    ("名詞", "普通名詞", "助数詞可能"): ("名詞,一般,*,*", 1285),
    ("名詞", "固有名詞", "一般"): ("名詞,固有名詞,一般,*", 1288),
    ("名詞", "固有名詞", "人名", "一般"): ("名詞,固有名詞,人名,一般", 1289),
    ("名詞", "固有名詞", "人名", "名"): ("名詞,固有名詞,人名,名", 1291),
    ("名詞", "固有名詞", "人名", "姓"): ("名詞,固有名詞,人名,姓", 1290),
    ("名詞", "固有名詞", "地名", "一般"): ("名詞,固有名詞,地域,一般", 1293),
    ("名詞", "固有名詞", "地名", "国"): ("名詞,固有名詞,地域,国", 1294),
    ("名詞", "数詞"): ("名詞,数,*,*", 1295),
    # 動詞系
    ("動詞", "一般"): ("動詞,自立,*,*", 619),
    ("動詞", "非自立可能"): ("動詞,非自立,*,*", 919),
    # 形容詞系
    ("形容詞", "一般"): ("形容詞,自立,*,*", 35),
    ("形容詞", "非自立可能"): ("形容詞,非自立,*,*", 129),
    # 形状詞 -> 名詞,形容動詞語幹
    ("形状詞", "一般"): ("名詞,形容動詞語幹,*,*", 1287),
    ("形状詞", "タリ"): ("名詞,形容動詞語幹,*,*", 1287),
    ("形状詞", "助動詞語幹"): ("名詞,形容動詞語幹,*,*", 1287),
    # 副詞
    ("副詞",): ("副詞,一般,*,*", 1281),
    # 連体詞
    ("連体詞",): ("連体詞,*,*,*", 1315),
    # 接続詞
    ("接続詞",): ("接続詞,*,*,*", 555),
    # 感動詞
    ("感動詞", "一般"): ("感動詞,*,*,*", 3),
    ("感動詞", "フィラー"): ("フィラー,*,*,*", 2),
    # 助詞系
    ("助詞", "格助詞"): ("助詞,格助詞,一般,*", 155),
    ("助詞", "係助詞"): ("助詞,係助詞,*,*", 258),
    ("助詞", "副助詞"): ("助詞,副助詞,*,*", 350),
    ("助詞", "接続助詞"): ("助詞,接続助詞,*,*", 307),
    ("助詞", "終助詞"): ("助詞,終助詞,*,*", 279),
    ("助詞", "準体助詞"): ("助詞,格助詞,一般,*", 155),
    # 助動詞
    ("助動詞",): ("助動詞,*,*,*", 483),
    # 接頭辞
    ("接頭辞",): ("接頭詞,名詞接続,*,*", 560),
    # 接尾辞系
    ("接尾辞", "名詞的", "一般"): ("名詞,接尾,一般,*", 1298),
    ("接尾辞", "名詞的", "サ変可能"): ("名詞,接尾,サ変接続,*", 1297),
    ("接尾辞", "名詞的", "副詞可能"): ("名詞,接尾,副詞可能,*", 1305),
    ("接尾辞", "名詞的", "助数詞"): ("名詞,接尾,助数詞,*", 1300),
    ("接尾辞", "動詞的"): ("動詞,接尾,*,*", 870),
    ("接尾辞", "形容詞的"): ("形容詞,接尾,*,*", 91),
    ("接尾辞", "形状詞的"): ("名詞,接尾,形容動詞語幹,*", 1299),
    # 代名詞
    ("代名詞",): ("名詞,代名詞,一般,*", 1306),
    # 記号系
    ("記号", "一般"): ("記号,一般,*,*", 5),
    ("記号", "文字"): ("記号,アルファベット,*,*", 4),
    ("補助記号", "一般"): ("記号,一般,*,*", 5),
    ("補助記号", "句点"): ("記号,句点,*,*", 8),
    ("補助記号", "読点"): ("記号,読点,*,*", 10),
    ("補助記号", "括弧開"): ("記号,括弧開,*,*", 6),
    ("補助記号", "括弧閉"): ("記号,括弧閉,*,*", 7),
    ("補助記号", "ＡＡ", "一般"): ("記号,一般,*,*", 5),
    ("補助記号", "ＡＡ", "顔文字"): ("記号,一般,*,*", 5),
    # 空白
    ("空白",): ("記号,空白,*,*", 9),
}


def lookup_mapping(pos1, pos2, pos3, pos4):
    """SudachiDict POSからIPAdic POS + IDを検索する.

    Returns:
        tuple: (ipadic_pos, ipadic_id) or None.

    """
    # 4要素から順に短いキーで検索
    keys = [
        (pos1, pos2, pos3, pos4),
        (pos1, pos2, pos3),
        (pos1, pos2),
        (pos1,),
    ]
    for key in keys:
        # *を除いたキーで検索
        clean_key = tuple(k for k in key if k != "*")
        if clean_key in SUDACHI_TO_IPADIC:
            return SUDACHI_TO_IPADIC[clean_key]
    return None


def remap_csv(input_csv, output_csv):
    """SudachiDict CSVをIPAdic互換にリマップする.

    Returns:
        tuple: (converted, skipped, unmapped_pos_set).

    """
    converted = 0
    skipped = 0
    unmapped = set()

    with (
        open(input_csv, encoding="utf-8") as fin,
        open(output_csv, "w", encoding="utf-8", newline="") as fout,
    ):
        reader = csv.reader(fin)
        writer = csv.writer(fout)

        for row in reader:
            if len(row) < 13:
                skipped += 1
                continue

            surface = row[0]
            cost = row[3]
            pos1, pos2, pos3, pos4 = row[4], row[5], row[6], row[7]
            conj_type, conj_form = row[8], row[9]
            base_form = row[10]
            reading = row[11]
            pronunciation = row[12]

            mapping = lookup_mapping(pos1, pos2, pos3, pos4)
            if mapping is None:
                unmapped.add(f"{pos1},{pos2},{pos3},{pos4}")
                skipped += 1
                continue

            ipadic_pos, ipadic_id = mapping
            pos_parts = ipadic_pos.split(",")
            while len(pos_parts) < 4:
                pos_parts.append("*")

            writer.writerow(
                [
                    surface,
                    ipadic_id,
                    ipadic_id,
                    cost,
                    pos_parts[0],
                    pos_parts[1],
                    pos_parts[2],
                    pos_parts[3],
                    conj_type,
                    conj_form,
                    base_form,
                    reading,
                    pronunciation,
                ]
            )
            converted += 1

    return converted, skipped, unmapped


def main():
    """SudachiDict CSVをIPAdic互換にリマップする."""
    parser = argparse.ArgumentParser(
        description="Remap SudachiDict CSV to IPAdic-compatible IDs"
    )
    parser.add_argument(
        "--input",
        required=True,
        help="Input SudachiDict CSV (MeCab format from convert script)",
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Output IPAdic-compatible CSV",
    )
    args = parser.parse_args()

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    converted, skipped, unmapped = remap_csv(args.input, args.output)

    print(f"Converted: {converted:,} entries", file=sys.stderr)
    print(f"Skipped:   {skipped:,} entries", file=sys.stderr)
    print(f"Output:    {args.output}", file=sys.stderr)

    if unmapped:
        print(f"\nUnmapped POS types ({len(unmapped)}):", file=sys.stderr)
        for pos in sorted(unmapped):
            print(f"  {pos}", file=sys.stderr)


if __name__ == "__main__":
    main()
