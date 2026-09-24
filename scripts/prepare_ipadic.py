"""配布辞書用に IPAdic のソースを整える.

上流の mecab-ipadic は書き換えず、`hasami build` に渡すディレクトリを別に作る。
辞書 CSV と matrix.def はリンクを張り、次の 5 点だけを変える。

1. 記号の未知語 (char.def・unk.def)
   IPAdic の char.def は U+2000..206F (— 等)・U+3000..303F (全角スペース・。、「」 等)・
   矢印・♪ などを SYMBOL (invoke=1 group=1) とし、unk.def は SYMBOL の未知語を
   「名詞,サ変接続」(コスト 17585) にしている。このままだと辞書に無い記号の並びが 1 つの名詞になり、
   「楽しみたい——。」の「——。」のように句点まで吸い込む。読み上げでも、句点・読点・括弧を
   品詞で見分ける Linter でも困るので、次のように変える。
     char.def  SYMBOL 1 1 0 → SYMBOL 0 0 0 (既知語がある位置では未知語を作らず、作るときも 1 文字ずつ)
     unk.def   SYMBOL → 記号,一般 (文脈 ID・コストは DEFAULT と同じ 5,5,4769)

2. 未知語の候補 (char.def)
   hasami は char.def を MeCab と同じ意味で読む (group なら同じ文字種の並び全体、続けて 1〜length 字の接頭辞)。
   IPAdic の値のままだと、ひらがなの並びが 1 つの名詞になり (「なき / ゃいけないってこともないし」)、
   英数字は 1 文字に分けられない (「04D」「08月」)。カタカナは並び全体を 1 語にしたい (「ブログ」
   「モチベーション」) ので IPAdic のまま (1 1 2) にし、ほかを次のように変える。
     char.def  HIRAGANA 0 1 2 → 0 0 2、ALPHA・NUMERIC 1 1 0 → 1 1 1
     char.def  中黒 U+30FB と × U+00D7・÷ U+00F7 を SYMBOL に (カタカナ・英字の並びを切る)

3. EUC-JP の変換差を埋める別表記 (variants.csv)
   IPAdic の CSV は EUC-JP で、JIS X 0208 のうち 7 字 (0xA1BD のダッシュ、0xA1C1 の波ダッシュ、
   0xA1DD のマイナスなど) は変換表によって写し先が分かれる。hasami は JIS の対応表どおり
   (iconv・MeCab と同じ)「—」U+2014「〜」U+301C「−」U+2212 等に写すが、Windows (CP932) 由来の
   文章は「―」U+2015「～」U+FF5E「－」U+FF0D 等を使う。表層形にこの 7 字を含む語に、CP932 側の字で
   書いた別表記を足す (「あ〜」に「あ～」、「——」に「――」)。品詞・文脈 ID・コスト・読みは元の語と同じで、
   原形が表層形と同じ語は原形も同じ字にする。入力の文字は変えない。

4. 空白の文字 (char.def)
   IPAdic の char.def は SPACE に 0x00D0 (Ð) を入れている。ほかの行 (0x0009・0x000A・0x000B) から
   見て復帰 0x000D の書き間違いなので、0x000D に直す。hasami は MeCab と同じく SPACE の文字を
   読み飛ばす (ノードにしない) ので、そのままだと「Ð」が解析結果から消える。

5. 単位の記号 (units.csv)
   全角の「％」は 名詞,接尾,助数詞 の語だが、半角の「%」と「‰」「℃」「°」、CJK 互換文字の単位 (「㎏」
   「㎞」「㌢」「㍍」など) は辞書に無く、未知の記号 (記号,一般) になる。数と単位をまとめる処理で単位が
   句読点と同じ記号として扱われるので、「％」と同じ品詞・文脈 ID・コストの語として読み付きで足す。

usage: python3 scripts/prepare_ipadic.py <mecab-ipadic のディレクトリ> <出力ディレクトリ>
       変えた内容を 1 行で標準出力に書く (辞書のメタデータ `ipadic_patch` に入れる)
"""

import sys
import unicodedata
from pathlib import Path

SYMBOL_CHAR_DEF = "SYMBOL 0 0 0"
SYMBOL_UNK_DEF = "SYMBOL,5,5,4769,記号,一般,*,*,*,*,*"
# 未知語の候補を MeCab から変えるカテゴリ (INVOKE GROUP LENGTH の意味は MeCab と同じ)
#   HIRAGANA 0 1 2 → 0 0 2: 並び全体を候補にしない。MeCab は既知語の無い位置 (小書きの「ゃ」など) から
#     続くひらがな (最大 25 字) を 1 つの名詞にする (「なき / ゃいけないってこともないし」)
#   ALPHA・NUMERIC 1 1 0 → 1 1 1: 並び全体に加えて 1 文字も候補にする。「04D」「08月」を
#     「0 / 4D」「0 / 8月」に分けて辞書の読みを使える (MeCab 式だと「04 / D」「08 / 月(ツキ)」)
CATEGORY_CHAR_DEF = {
    "HIRAGANA": "HIRAGANA 0 0 2",
    "ALPHA": "ALPHA 1 1 1",
    "NUMERIC": "NUMERIC 1 1 1",
}
# char.def の末尾に足す文字の割り当て (hasami は文字を含む範囲のうち開始位置が最も大きいものを使う)
#   中黒は KATAKANA の範囲にあり、「ジョン・カーター」が 1 つの未知語になって読みを補えない (無音になる)
#   × ÷ は ALPHA の範囲 0x00C0..0x00FF にあり、「microSD×C」「NIN×NIN」が 1 つの英字の未知語になる
EXTRA_CHAR_RANGES = [
    "0x30FB SYMBOL  # KATAKANA MIDDLE DOT",
    "0x00D7 SYMBOL  # MULTIPLICATION SIGN",
    "0x00F7 SYMBOL  # DIVISION SIGN",
]
# IPAdic の char.def は SPACE に 0x00D0 (Ð) を入れている。ほかの行 (0x0009・0x000A・0x000B) から見て、
# 復帰 0x000D の書き間違い。hasami は MeCab と同じく SPACE の文字を読み飛ばすので、そのままだと Ð が消える
MISTYPED_SPACE = "0x00D0"
SPACE_FIX = "0x000D SPACE  # CARRIAGE RETURN (IPAdic の 0x00D0 は 0x000D の書き間違い)"

# 数に付く単位の記号 (units.csv)。IPAdic の全角の「％」と同じ 名詞,接尾,助数詞 にする (文脈 ID・コストは「％」の行から
# 取る)。表層形 → 読み
UNIT_SYMBOLS = {
    "%": "パーセント",
    "‰": "パーミル",
    "℃": "ド",
    "℉": "ド",
    "°": "ド",
    "°C": "ド",
    "°F": "ド",
}
# CJK 互換文字のカタカナの組文字 (U+3300..U+3357) は、読みを NFKC の形 (「㌢」→「センチ」) から取る。
# 単位・通貨・接頭辞でない字 (アパート アルファ ガンマ コーポ ハイツ ビル ベータ ホール マンション) は除く
KATAKANA_SQUARES = range(0x3300, 0x3358)
NOT_UNIT_KATAKANA_SQUARES = set("㌀㌁㌏㌞㌪㌱㌼㍁㍇")
# CJK 互換文字のラテン文字の組文字の単位。午前・午後・株式会社・対数など単位でない字と、読みが分かれる字
# (㏏ kt はノットかカラット、㏿ gal はガロンかガル、㍲ da は接頭辞) は入れない
LATIN_UNIT_SQUARES = {
    "㍱": "ヘクトパスカル",
    "㍳": "エーユー",
    "㍴": "バール",
    "㍶": "パーセク",
    "㍷": "デシメートル",
    "㍸": "ヘイホウデシメートル",
    "㍹": "リッポウデシメートル",
    "㍺": "アイユー",
    "㎀": "ピコアンペア",
    "㎁": "ナノアンペア",
    "㎂": "マイクロアンペア",
    "㎃": "ミリアンペア",
    "㎄": "キロアンペア",
    "㎅": "キロバイト",
    "㎆": "メガバイト",
    "㎇": "ギガバイト",
    "㎈": "カロリー",
    "㎉": "キロカロリー",
    "㎊": "ピコファラド",
    "㎋": "ナノファラド",
    "㎌": "マイクロファラド",
    "㎍": "マイクログラム",
    "㎎": "ミリグラム",
    "㎏": "キログラム",
    "㎐": "ヘルツ",
    "㎑": "キロヘルツ",
    "㎒": "メガヘルツ",
    "㎓": "ギガヘルツ",
    "㎔": "テラヘルツ",
    "㎕": "マイクロリットル",
    "㎖": "ミリリットル",
    "㎗": "デシリットル",
    "㎘": "キロリットル",
    "㎙": "フェムトメートル",
    "㎚": "ナノメートル",
    "㎛": "マイクロメートル",
    "㎜": "ミリメートル",
    "㎝": "センチメートル",
    "㎞": "キロメートル",
    "㎟": "ヘイホウミリメートル",
    "㎠": "ヘイホウセンチメートル",
    "㎡": "ヘイホウメートル",
    "㎢": "ヘイホウキロメートル",
    "㎣": "リッポウミリメートル",
    "㎤": "リッポウセンチメートル",
    "㎥": "リッポウメートル",
    "㎦": "リッポウキロメートル",
    "㎧": "メートルマイビョウ",
    "㎨": "メートルマイビョウマイビョウ",
    "㎩": "パスカル",
    "㎪": "キロパスカル",
    "㎫": "メガパスカル",
    "㎬": "ギガパスカル",
    "㎭": "ラジアン",
    "㎮": "ラジアンマイビョウ",
    "㎯": "ラジアンマイビョウマイビョウ",
    "㎰": "ピコビョウ",
    "㎱": "ナノビョウ",
    "㎲": "マイクロビョウ",
    "㎳": "ミリビョウ",
    "㎴": "ピコボルト",
    "㎵": "ナノボルト",
    "㎶": "マイクロボルト",
    "㎷": "ミリボルト",
    "㎸": "キロボルト",
    "㎹": "メガボルト",
    "㎺": "ピコワット",
    "㎻": "ナノワット",
    "㎼": "マイクロワット",
    "㎽": "ミリワット",
    "㎾": "キロワット",
    "㎿": "メガワット",
    "㏀": "キロオーム",
    "㏁": "メガオーム",
    "㏃": "ベクレル",
    "㏄": "シーシー",
    "㏅": "カンデラ",
    "㏆": "クーロンマイキログラム",
    "㏈": "デシベル",
    "㏉": "グレイ",
    "㏊": "ヘクタール",
    "㏋": "バリキ",
    "㏌": "インチ",
    "㏎": "キロメートル",
    "㏐": "ルーメン",
    "㏓": "ルクス",
    "㏔": "ミリバール",
    "㏕": "ミル",
    "㏖": "モル",
    "㏙": "ピーピーエム",
    "㏛": "ステラジアン",
    "㏜": "シーベルト",
    "㏝": "ウェーバ",
    "㏞": "ボルトマイメートル",
    "㏟": "アンペアマイメートル",
}

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
    replace = {"SYMBOL": SYMBOL_CHAR_DEF, **CATEGORY_CHAR_DEF}
    found = set()
    space_fixed = False
    for line in lines:
        fields = line.split()
        if (
            fields
            and fields[0] in replace
            and len(fields) >= 4
            and not fields[1].startswith("0x")
        ):
            out.append(replace[fields[0]])
            found.add(fields[0])
        elif fields[:2] == [MISTYPED_SPACE, "SPACE"]:
            out.append(SPACE_FIX)
            space_fixed = True
        else:
            out.append(line)
    missing = replace.keys() - found
    if missing:
        sys.exit(f"char.def: categories not found: {sorted(missing)}")
    if not space_fixed:
        sys.exit(f"char.def: {MISTYPED_SPACE} SPACE not found")
    return "\n".join([*out, *EXTRA_CHAR_RANGES]) + "\n"


def patch_unk_def(text: str) -> str:
    lines = text.splitlines()
    out = [line for line in lines if not line.startswith("SYMBOL,")]
    if len(out) == len(lines):
        sys.exit("unk.def: SYMBOL template not found")
    # SYMBOL のテンプレートを 記号,一般 の 1 つにする (IPAdic の SYMBOL のテンプレートも 1 つ)
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


def unit_entries(src: Path) -> list[str]:
    """単位の記号を、IPAdic の「％」(名詞,接尾,助数詞) と同じ文脈 ID・コストの語にする"""
    template = None
    for path in sorted(src.glob("*.csv")):
        for line in decode_like_hasami(path.read_bytes()).splitlines():
            fields = line.split(",")
            if fields[0] == "％" and fields[4:7] == ["名詞", "接尾", "助数詞"]:
                template = fields
    if template is None:
        sys.exit("IPAdic: ％ (名詞,接尾,助数詞) not found")
    units = dict(UNIT_SYMBOLS)
    for cp in KATAKANA_SQUARES:
        c = chr(cp)
        if c not in NOT_UNIT_KATAKANA_SQUARES:
            units[c] = unicodedata.normalize("NFKC", c)
    units.update(LATIN_UNIT_SQUARES)
    return [
        ",".join([surface, *template[1:10], surface, reading, reading])
        for surface, reading in units.items()
    ]


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
    units = unit_entries(src)
    (out / "units.csv").write_text(
        "".join(f"{row}\n" for row in units), encoding="utf-8"
    )

    print(
        f"char.def {SYMBOL_CHAR_DEF}, {', '.join(CATEGORY_CHAR_DEF.values())}, "
        f"U+30FB/U+00D7/U+00F7 SYMBOL, 0x000D SPACE (not 0x00D0); unk.def SYMBOL=記号,一般; "
        f"{len(variants)} CP932-side variants of dashes, tildes and minus signs; "
        f"{len(units)} unit symbols as 名詞,接尾,助数詞"
    )


if __name__ == "__main__":
    main()
