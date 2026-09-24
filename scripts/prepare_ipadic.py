"""配布辞書用に IPAdic のソースを整える.

上流の mecab-ipadic は書き換えず、`hasami build` に渡すディレクトリを別に作る。
辞書 CSV と matrix.def はリンクを張り、次の 2 点だけを変える。

1. 記号の未知語 (char.def・unk.def)
   IPAdic の char.def は U+2000..206F (— 等)・U+3000..303F (全角スペース・。、「」 等)・
   矢印・♪ などを SYMBOL (invoke=1 group=1) とし、unk.def は SYMBOL の未知語を
   「名詞,サ変接続」(コスト 17585) にしている。このままだと辞書に無い記号の並びが 1 つの名詞になり、
   「楽しみたい——。」の「——。」のように句点まで吸い込む。読み上げでも、句点・読点・括弧を
   品詞で見分ける Linter でも困るので、次のように変える。
     char.def  SYMBOL 1 1 0 → SYMBOL 0 0 0 (既知語がある位置では未知語を作らず、作るときも 1 文字ずつ)
     unk.def   SYMBOL → 記号,一般 (文脈 ID・コストは DEFAULT と同じ 5,5,4769)

2. EUC-JP の変換差を埋める別表記 (variants.csv)
   IPAdic の CSV は EUC-JP で、JIS X 0208 のうち 7 字 (0xA1BD のダッシュ、0xA1C1 の波ダッシュ、
   0xA1DD のマイナスなど) は変換表によって写し先が分かれる。hasami は JIS の対応表どおり
   (iconv・MeCab と同じ)「—」U+2014「〜」U+301C「−」U+2212 等に写すが、Windows (CP932) 由来の
   文章は「―」U+2015「～」U+FF5E「－」U+FF0D 等を使う。表層形にこの 7 字を含む語に、CP932 側の字で
   書いた別表記を足す (「あ〜」に「あ～」、「——」に「――」)。品詞・文脈 ID・コスト・読みは元の語と同じで、
   原形が表層形と同じ語は原形も同じ字にする。入力の文字は変えない。

usage: python3 scripts/prepare_ipadic.py <mecab-ipadic のディレクトリ> <出力ディレクトリ>
       変えた内容を 1 行で標準出力に書く (辞書のメタデータ `ipadic_patch` に入れる)
"""

import sys
from pathlib import Path

SYMBOL_CHAR_DEF = "SYMBOL 0 0 0"
SYMBOL_UNK_DEF = "SYMBOL,5,5,4769,記号,一般,*,*,*,*,*"

# 変換表によって写し先が分かれる JIS X 0208 の字: JIS 側 (hasami・iconv) → CP932 側 (Windows)
CP932_SIDE = {
    "\u2014": "\u2015",  # — → ― (0xA1BD)
    "\u301c": "\uff5e",  # 〜 → ～ (0xA1C1)
    "\u2016": "\u2225",  # ‖ → ∥ (0xA1C2)
    "\u2212": "\uff0d",  # − → － (0xA1DD)
    "\u00a2": "\uffe0",  # ¢ → ￠ (0xA1F1)
    "\u00a3": "\uffe1",  # £ → ￡ (0xA1F2)
    "\u00ac": "\uffe2",  # ¬ → ￢ (0xA2CC)
}
# Python の euc_jp は 0xA1BD だけ CP932 側 (U+2015) に写すので、hasami と同じ JIS 側に直す
PYTHON_TO_JIS = {"\u2015": "\u2014"}


def read_text(path: Path) -> str:
    raw = path.read_bytes()
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return raw.decode("euc_jp")


def decode_like_hasami(raw: bytes) -> str:
    """hasami と同じ字 (7 字は JIS 側) になるように EUC-JP を読む"""
    text = raw.decode("euc_jp")
    return "".join(PYTHON_TO_JIS.get(c, c) for c in text)


def cp932_side(s: str) -> str:
    return "".join(CP932_SIDE.get(c, c) for c in s)


def patch_char_def(text: str) -> str:
    lines = text.splitlines()
    out = []
    found = False
    for line in lines:
        fields = line.split()
        if (
            fields
            and fields[0] == "SYMBOL"
            and len(fields) >= 4
            and not fields[1].startswith("0x")
        ):
            out.append(SYMBOL_CHAR_DEF)
            found = True
        else:
            out.append(line)
    if not found:
        sys.exit("char.def: SYMBOL category not found")
    return "\n".join(out) + "\n"


def patch_unk_def(text: str) -> str:
    lines = text.splitlines()
    out = [line for line in lines if not line.startswith("SYMBOL,")]
    if len(out) == len(lines):
        sys.exit("unk.def: SYMBOL template not found")
    # SYMBOL のテンプレートは 1 つにする (hasami は文字種ごとに先頭の 1 つだけを使う)
    return "\n".join([*out, SYMBOL_UNK_DEF]) + "\n"


def variant_entries(src: Path) -> list[str]:
    rows = []
    for path in sorted(src.glob("*.csv")):
        for line in decode_like_hasami(path.read_bytes()).splitlines():
            fields = line.split(",")
            surface = fields[0]
            variant = cp932_side(surface)
            if variant == surface:
                continue
            # 原形 (11 列目) が表層形と同じなら、原形も同じ字にする
            if len(fields) > 10 and fields[10] == surface:
                fields[10] = variant
            rows.append(",".join([variant, *fields[1:]]))
    return rows


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    src = Path(sys.argv[1]).resolve()
    out = Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    for old in out.iterdir():
        if old.is_symlink() or old.suffix in {".csv", ".def"}:
            old.unlink()

    for path in sorted(src.glob("*.csv")):
        (out / path.name).symlink_to(path)
    (out / "matrix.def").symlink_to(src / "matrix.def")
    (out / "char.def").write_text(
        patch_char_def(read_text(src / "char.def")), encoding="utf-8"
    )
    (out / "unk.def").write_text(
        patch_unk_def(read_text(src / "unk.def")), encoding="utf-8"
    )
    variants = variant_entries(src)
    (out / "variants.csv").write_text(
        "".join(f"{row}\n" for row in variants), encoding="utf-8"
    )

    print(
        f"char.def {SYMBOL_CHAR_DEF}; unk.def SYMBOL=記号,一般; "
        f"{len(variants)} CP932-side variants of dashes, tildes and minus signs"
    )


if __name__ == "__main__":
    main()
