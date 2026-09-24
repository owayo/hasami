r"""SudachiDict の公式 raw 辞書ソース (V1 形式 CSV) を hasami 用 MeCab 形式 CSV に変換する.

IPAdic + NEologd の辞書に SudachiDict の語彙を追加するための変換スクリプト。
品詞を IPAdic 体系に写し、文脈 ID は IPAdic の left-id.def から
(品詞, 活用型, 活用形, rewrite.def による語彙化) の組で引く。
対応する ID が無い品詞・活用形は代表 ID で代用せず、取り込まない。

入力 (http://sudachi.s3-website-ap-northeast-1.amazonaws.com/sudachidict-raw/v1/ から取得):
    small_lex.csv, core_lex.csv (任意で notcore_lex.csv)

列の扱い:
    表層形   Headword (空なら IndexForm)。IndexForm は Sudachi の入力正規化
             (小文字化・NFKC) 後の表記で、正規化しない hasami では原表記の方が当たる
    文脈 ID  IPAdic の left-id.def (= right-id.def) から引く
    コスト   Sudachi のコスト。--cost-scale / --cost-offset で線形変換できる
    品詞     UniDic 系 → IPAdic 系 (POS_MAP)
    活用     UniDic の活用型・活用形 → IPAdic の活用型・活用形。
             原形から作った IPAdic の活用表と表層形が一致するものだけ採る
    原形     DictionaryForm が指す辞書形の見出し (空なら自分自身)
    読み     ReadingForm。記号の「キゴウ」は IPAdic に合わせて表層形にする
    発音     読みと同じ

取り込まないもの:
    LeftId = -1 の語 (分割情報の構成語専用で、単独では出現しない)
    文語の活用語、助詞・助動詞 (IPAdic の閉じた語彙で、語ごとに ID が異なる)
    IPAdic の活用表に無い活用形 (「回ろう」「回ん(連用)」「高っ」等)
    --exclude-existing に渡した辞書と (表層形, 品詞大分類, 読み) が同じ語

Usage:
    python3 scripts/convert_sudachi_raw.py \
        --lex .dict-src/sudachi-raw/small_lex.csv \
        --lex .dict-src/sudachi-raw/core_lex.csv \
        --ipadic-dir .dict-src/mecab/mecab-ipadic \
        --exclude-existing .dict-src/mecab/mecab-ipadic \
        --exclude-existing .dict-src/neologd-seed \
        --scope noun \
        --output .dict-src/sudachi-converted/sudachi.csv
"""

import argparse
import collections
import csv
import re
import sys
import time
from pathlib import Path

# ---------------------------------------------------------------------------
# 入力の読み込み
# ---------------------------------------------------------------------------

_ESCAPE = re.compile(r"\\u(?:\{([0-9a-fA-F]{1,6})\}|([0-9a-fA-F]{4}))")

# V1 形式のヘッダー名 (大文字小文字・"_" を無視) → 内部名
_COLUMN_ALIASES = {
    "indexform": "index",
    "surface": "index",
    "leftid": "left",
    "rightid": "right",
    "cost": "cost",
    "headword": "headword",
    "writing": "headword",
    "pos1": "pos1",
    "pos2": "pos2",
    "pos3": "pos3",
    "pos4": "pos4",
    "pos5": "pos5",
    "pos6": "pos6",
    "readingform": "reading",
    "normalizedform": "normalized",
    "dictionaryform": "dictionary",
}
_REQUIRED = ("index", "left", "right", "cost", "pos1", "pos2", "pos3", "pos4")
_REQUIRED += ("pos5", "pos6", "reading", "dictionary")


def unescape(value):
    r"""Sudachi の \uXXXX / \u{X..} エスケープを戻す."""
    if "\\u" not in value:
        return value
    return _ESCAPE.sub(lambda m: chr(int(m.group(1) or m.group(2), 16)), value)


def read_lexicon(path):
    """V1 形式の lexicon CSV を 1 行ずつ dict で返す (列はヘッダー名で引く)."""
    with open(path, encoding="utf-8", newline="") as f:
        reader = csv.reader(f)
        header = next(reader)
        columns = {}
        for i, name in enumerate(header):
            key = _COLUMN_ALIASES.get(name.replace("_", "").lower())
            if key is not None:
                columns[key] = i
        missing = [c for c in _REQUIRED if c not in columns]
        if missing:
            sys.exit(f"{path}: V1 形式のヘッダーに {missing} がありません: {header}")
        for row in reader:
            yield {key: row[i] if i < len(row) else "" for key, i in columns.items()}


def split_reference(ref):
    """語参照 "見出し,品詞1..6,読み[,参照ID]" を (見出し, 品詞6つ組, 読み) にする."""
    parts = ref.split(",")
    if len(parts) < 8:
        return None
    return unescape(parts[0]), tuple(parts[1:7]), unescape(parts[7])


def decode_bytes(raw):
    """UTF-8 でなければ EUC-JP (IPAdic 配布物) として読む."""
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return raw.decode("euc_jp")


def csv_files(path):
    """ファイルならそのもの、ディレクトリなら直下の *.csv を名前順で返す."""
    p = Path(path)
    return sorted(p.glob("*.csv")) if p.is_dir() else [p]


# ---------------------------------------------------------------------------
# IPAdic の文脈 ID
# ---------------------------------------------------------------------------


class ContextIds:
    """IPAdic の rewrite.def ([left rewrite]) と left-id.def で文脈 ID を引く.

    IPAdic は left-id.def と right-id.def が同一なので、左右とも同じ ID を使う。
    """

    def __init__(self, ipadic_dir):
        """IPAdic ソースディレクトリから left-id.def と rewrite.def を読む."""
        d = Path(ipadic_dir)
        self.ids = {}
        for line in decode_bytes((d / "left-id.def").read_bytes()).splitlines():
            if line.strip():
                num, feature = line.split(" ", 1)
                self.ids[feature.strip()] = int(num)
        right = decode_bytes((d / "right-id.def").read_bytes()).splitlines()
        right_ids = {}
        for line in right:
            if line.strip():
                num, feature = line.split(" ", 1)
                right_ids[feature.strip()] = int(num)
        if right_ids != self.ids:
            sys.exit(f"{d}: left-id.def と right-id.def が一致しません")
        self.rules = self._read_rewrite(d / "rewrite.def")

    @staticmethod
    def _read_rewrite(path):
        rules = []
        section = None
        for line in decode_bytes(path.read_bytes()).splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if line.startswith("["):
                section = line
                continue
            if section != "[left rewrite]":
                continue
            pattern, output = line.split()
            fields = []
            for pat in pattern.split(","):
                if pat.startswith("(") and pat.endswith(")"):
                    fields.append(frozenset(pat[1:-1].split("|")))
                else:
                    fields.append(pat)
            rules.append((fields, output.split(",")))
        return rules

    @staticmethod
    def _match(pat, value):
        if isinstance(pat, frozenset):
            return value in pat
        return pat == "*" or pat == value

    def lookup(self, pos, ctype, cform, base):
        """(品詞4つ組, 活用型, 活用形, 原形) の文脈 ID。無ければ None."""
        feature = (*pos, ctype, cform, base)
        for pattern, output in self.rules:
            if len(pattern) > len(feature):
                continue
            if all(self._match(p, v) for p, v in zip(pattern, feature)):
                rewritten = []
                for out in output:
                    if out.startswith("$"):
                        rewritten.append(feature[int(out[1:]) - 1])
                    else:
                        rewritten.append(out)
                return self.ids.get(",".join(rewritten))
        return None


# ---------------------------------------------------------------------------
# 品詞の対応 (UniDic 系 → IPAdic 系)
# ---------------------------------------------------------------------------

# (Sudachi 品詞1〜4 の先頭一致キー) → (取り込み区分, IPAdic 品詞4つ組)
# 取り込まないもの (助詞・助動詞・助動詞語幹) は対応を置かない。
# 対応先は IPAdic と Sudachi の双方にある語の品詞の多数派で選んだ。
POS_MAP = {
    ("名詞", "普通名詞", "一般"): ("noun", "名詞,一般,*,*"),
    ("名詞", "普通名詞", "サ変可能"): ("noun", "名詞,サ変接続,*,*"),
    ("名詞", "普通名詞", "形状詞可能"): ("noun", "名詞,形容動詞語幹,*,*"),
    ("名詞", "普通名詞", "サ変形状詞可能"): ("noun", "名詞,サ変接続,*,*"),
    ("名詞", "普通名詞", "副詞可能"): ("noun", "名詞,副詞可能,*,*"),
    ("名詞", "普通名詞", "助数詞可能"): ("noun", "名詞,一般,*,*"),
    ("名詞", "固有名詞", "一般"): ("proper", "名詞,固有名詞,一般,*"),
    ("名詞", "固有名詞", "人名", "一般"): ("proper", "名詞,固有名詞,人名,一般"),
    ("名詞", "固有名詞", "人名", "姓"): ("proper", "名詞,固有名詞,人名,姓"),
    ("名詞", "固有名詞", "人名", "名"): ("proper", "名詞,固有名詞,人名,名"),
    ("名詞", "固有名詞", "地名", "一般"): ("proper", "名詞,固有名詞,地域,一般"),
    ("名詞", "固有名詞", "地名", "国"): ("proper", "名詞,固有名詞,地域,国"),
    # 数字 1 文字ごとの語 (「2(ニ)」「0(レイ)」) が IPAdic の未知語処理 (数字の連続を
    # 1 語にまとめる) に勝ち、「2012年」を「ニ レイ イチ ニ ネン」と読ませるため取り込まない
    ("名詞", "数詞"): ("numeral", "名詞,数,*,*"),
    ("代名詞",): ("pronoun", "名詞,代名詞,一般,*"),
    ("形状詞", "一般"): ("adjv", "名詞,形容動詞語幹,*,*"),
    ("形状詞", "タリ"): ("adjv", "名詞,一般,*,*"),
    ("連体詞",): ("adnominal", "連体詞,*,*,*"),
    ("副詞",): ("adverb", "副詞,一般,*,*"),
    ("接続詞",): ("conjunction", "接続詞,*,*,*"),
    ("感動詞", "一般"): ("interjection", "感動詞,*,*,*"),
    ("感動詞", "フィラー"): ("interjection", "フィラー,*,*,*"),
    ("動詞", "一般"): ("verb", "動詞,自立,*,*"),
    ("動詞", "非自立可能"): ("verb", "動詞,自立,*,*"),
    ("形容詞", "一般"): ("adjective", "形容詞,自立,*,*"),
    ("形容詞", "非自立可能"): ("adjective", "形容詞,自立,*,*"),
    ("接頭辞",): ("prefix", "接頭詞,名詞接続,*,*"),
    ("接尾辞", "名詞的", "一般"): ("suffix", "名詞,接尾,一般,*"),
    ("接尾辞", "名詞的", "サ変可能"): ("suffix", "名詞,接尾,サ変接続,*"),
    ("接尾辞", "名詞的", "副詞可能"): ("suffix", "名詞,接尾,副詞可能,*"),
    ("接尾辞", "名詞的", "助数詞"): ("suffix", "名詞,接尾,助数詞,*"),
    ("接尾辞", "形状詞的"): ("suffix", "名詞,接尾,形容動詞語幹,*"),
    ("接尾辞", "動詞的"): ("suffix", "動詞,接尾,*,*"),
    ("接尾辞", "形容詞的"): ("suffix", "形容詞,接尾,*,*"),
    ("補助記号", "一般"): ("symbol", "記号,一般,*,*"),
    ("補助記号", "句点"): ("symbol", "記号,句点,*,*"),
    ("補助記号", "読点"): ("symbol", "記号,読点,*,*"),
    ("補助記号", "括弧開"): ("symbol", "記号,括弧開,*,*"),
    ("補助記号", "括弧閉"): ("symbol", "記号,括弧閉,*,*"),
    ("補助記号", "ＡＡ"): ("symbol", "記号,一般,*,*"),
    ("記号", "一般"): ("symbol", "記号,一般,*,*"),
    ("記号", "文字"): ("symbol", "記号,アルファベット,*,*"),
    ("空白",): ("symbol", "記号,空白,*,*"),
}

SCOPES = {
    "all": {
        "noun",
        "proper",
        "pronoun",
        "adjv",
        "adnominal",
        "adverb",
        "conjunction",
        "interjection",
        "verb",
        "adjective",
        "prefix",
        "suffix",
        "symbol",
    },
    "content": {
        "noun",
        "proper",
        "adjv",
        "adnominal",
        "adverb",
        "conjunction",
        "interjection",
        "verb",
        "adjective",
    },
    "noun": {"noun", "proper"},
    "proper": {"proper"},
}
SCOPES["content-symbol"] = SCOPES["content"] | {"symbol"}


def map_pos(pos):
    """Sudachi の品詞6つ組を (取り込み区分, IPAdic 品詞4つ組) に写す. 無ければ None."""
    fields = [p for p in pos[:4] if p != "*"]
    for n in range(len(fields), 0, -1):
        hit = POS_MAP.get(tuple(fields[:n]))
        if hit is not None:
            return hit[0], tuple(hit[1].split(","))
    return None


# ---------------------------------------------------------------------------
# 活用 (UniDic 系 → IPAdic 系)
# ---------------------------------------------------------------------------


def _godan(u, a, o, i, e, ya, ta=None, **extra):
    table = {
        "基本形": (u,),
        "未然形": (a,),
        "未然ウ接続": (o,),
        "連用形": (i,),
        "仮定形": (e,),
        "命令ｅ": (e,),
    }
    if ya:
        table["仮定縮約１"] = (ya,)
    if ta:
        table["連用タ接続"] = (ta,)
    table.update({k: (v,) for k, v in extra.items()})
    return u, table


_ADJ = {
    "基本形": ("い",),
    "文語基本形": ("し",),
    "未然ヌ接続": ("から",),
    "未然ウ接続": ("かろ",),
    "連用タ接続": ("かっ",),
    "連用テ接続": ("く", "くっ"),
    "仮定形": ("けれ",),
    "仮定縮約１": ("けりゃ",),
    "仮定縮約２": ("きゃ",),
    "体言接続": ("き",),
    "命令ｅ": ("かれ",),
    "ガル接続": ("",),
}

# IPAdic 活用型 → (原形の語尾, {活用形: 語幹に続く表記})。IPAdic の CSV の実データから作った
PARADIGMS = {
    "五段・カ行イ音便": _godan("く", "か", "こ", "き", "け", "きゃ", ta="い"),
    "五段・カ行促音便": _godan("く", "か", "こ", "き", "け", "きゃ", ta="っ"),
    "五段・カ行促音便ユク": _godan("く", "か", "こ", "き", "け", "きゃ"),
    "五段・ガ行": _godan("ぐ", "が", "ご", "ぎ", "げ", "ぎゃ", ta="い"),
    "五段・サ行": _godan("す", "さ", "そ", "し", "せ", "しゃ"),
    "五段・タ行": _godan("つ", "た", "と", "ち", "て", "ちゃ", ta="っ"),
    "五段・ナ行": _godan("ぬ", "な", "の", "に", "ね", "にゃ", ta="ん"),
    "五段・バ行": _godan("ぶ", "ば", "ぼ", "び", "べ", "びゃ", ta="ん"),
    "五段・マ行": _godan("む", "ま", "も", "み", "め", "みゃ", ta="ん"),
    "五段・ラ行": _godan(
        "る",
        "ら",
        "ろ",
        "り",
        "れ",
        "りゃ",
        ta="っ",
        未然特殊="ん",
        体言接続特殊="ん",
    ),
    "五段・ラ行特殊": _godan(
        "る", "ら", "ろ", "い", "れ", "りゃ", ta="っ", 未然特殊="ん", 命令ｉ="い"
    ),
    "五段・ワ行促音便": _godan("う", "わ", "お", "い", "え", None, ta="っ"),
    "五段・ワ行ウ音便": _godan("う", "わ", "お", "い", "え", None, ta="う"),
    "一段": (
        "る",
        {
            "基本形": ("る",),
            "未然形": ("",),
            "連用形": ("",),
            "未然ウ接続": ("よ",),
            "仮定形": ("れ",),
            "命令ｒｏ": ("ろ",),
            "命令ｙｏ": ("よ",),
            "仮定縮約１": ("りゃ",),
            "体言接続特殊": ("ん",),
        },
    ),
    "カ変・来ル": (
        "る",
        {
            "基本形": ("る",),
            "未然形": ("",),
            "連用形": ("",),
            "未然ウ接続": ("よ",),
            "命令ｙｏ": ("よ",),
            "命令ｉ": ("い",),
            "仮定形": ("れ",),
            "仮定縮約１": ("りゃ",),
            "体言接続特殊": ("ん",),
        },
    ),
    "カ変・クル": (
        "くる",
        {
            "基本形": ("くる",),
            "未然形": ("こ",),
            "連用形": ("き",),
            "未然ウ接続": ("こよ",),
            "命令ｙｏ": ("こよ",),
            "命令ｉ": ("こい",),
            "仮定形": ("くれ",),
            "仮定縮約１": ("くりゃ",),
            "体言接続特殊": ("くん",),
        },
    ),
    "サ変・−スル": (
        "する",
        {
            "基本形": ("する",),
            "未然形": ("し",),
            "未然レル接続": ("せ",),
            "未然ウ接続": ("しよ", "しょ"),
            "仮定形": ("すれ",),
            "命令ｒｏ": ("しろ",),
            "命令ｙｏ": ("せよ",),
            "文語基本形": ("す",),
            "仮定縮約１": ("すりゃ",),
        },
    ),
    "サ変・−ズル": (
        "ずる",
        {
            "基本形": ("ずる",),
            "未然形": ("ぜ",),
            "未然ウ接続": ("ぜよ",),
            "仮定形": ("ずれ",),
            "命令ｙｏ": ("ぜよ",),
            "文語基本形": ("ず",),
            "仮定縮約１": ("ずりゃ",),
        },
    ),
    "形容詞・アウオ段": ("い", {**_ADJ, "連用ゴザイ接続": ("う", "ぅ")}),
    "形容詞・イ段": ("い", {**_ADJ, "文語基本形": ("",), "連用ゴザイ接続": ("ゅう",)}),
}

# UniDic の活用形 → 対応しうる IPAdic の活用形 (先に書いたものを優先)。
# ここに無い活用形 (連用形-融合・終止形-撥音便・語幹-サ・已然形 等) は取り込まない。
CFORM_MAP = {
    "終止形-一般": ("基本形",),
    "連体形-一般": ("基本形",),
    "未然形-一般": ("未然形", "未然ヌ接続"),
    "未然形-セ": ("未然レル接続", "未然形"),
    "未然形-撥音便": ("未然特殊",),
    "意志推量形": ("未然ウ接続",),
    "連用形-一般": ("連用形", "連用テ接続", "未然形"),
    "連用形-促音便": ("連用タ接続",),
    "連用形-イ音便": ("連用タ接続", "連用形"),
    "連用形-撥音便": ("連用タ接続",),
    "連用形-ウ音便": ("連用タ接続", "連用ゴザイ接続"),
    "仮定形-一般": ("仮定形",),
    "仮定形-融合": ("仮定縮約１", "仮定縮約２"),
    "命令形": ("命令ｅ", "命令ｒｏ", "命令ｙｏ", "命令ｉ"),
    "連体形-撥音便": ("体言接続特殊",),
    "語幹-一般": ("ガル接続",),
}

_I_ROW = set("イキシチニヒミリギジヂビピィ")

# IPAdic が語ごとの専用の活用型を持つ動詞 (IPAdic 自身に収録済み)
_SPECIAL_VERBS = {"する", "為る", "くれる", "呉れる", "得る", "える"}


def ipadic_ctype(pos1, ctype, base, base_reading, cforms):
    """UniDic の活用型と語彙素の情報から IPAdic の活用型を決める. 決められなければ None."""
    if ctype.startswith("文語") or base in _SPECIAL_VERBS:
        return None
    if pos1 in ("形容詞", "接尾辞") and ctype == "形容詞":
        if not base_reading.endswith("イ") or len(base_reading) < 2:
            return None
        if base_reading[-2] in _I_ROW:
            return "形容詞・イ段"
        return "形容詞・アウオ段"
    if ctype.startswith(("上一段-", "下一段-")):
        return "一段"
    if ctype == "カ行変格":
        if base.endswith("来る"):
            return "カ変・来ル"
        if base.endswith("くる"):
            return "カ変・クル"
        return None
    if ctype == "サ行変格":
        if base.endswith("ずる"):
            return "サ変・−ズル"
        if base.endswith("する"):
            return "サ変・−スル"
        return None
    if ctype == "五段-カ行":
        if base_reading.endswith("ユク"):
            return "五段・カ行促音便ユク"
        if "連用形-促音便" in cforms and "連用形-イ音便" not in cforms:
            return "五段・カ行促音便"
        return "五段・カ行イ音便"
    if ctype == "五段-ラ行":
        return "五段・ラ行特殊" if "連用形-イ音便" in cforms else "五段・ラ行"
    if ctype == "五段-ワア行":
        if "連用形-促音便" not in cforms and "連用形-ウ音便" in cforms:
            return "五段・ワ行ウ音便"
        return "五段・ワ行促音便"
    simple = {
        "五段-ガ行": "五段・ガ行",
        "五段-サ行": "五段・サ行",
        "五段-タ行": "五段・タ行",
        "五段-ナ行": "五段・ナ行",
        "五段-バ行": "五段・バ行",
        "五段-マ行": "五段・マ行",
    }
    return simple.get(ctype)


def ipadic_cform(ctype, cform, surface, base):
    """表層形が IPAdic の活用表のどの活用形に当たるか. 当たらなければ None."""
    ending, table = PARADIGMS[ctype]
    if not base.endswith(ending):
        return None
    stem = base[: len(base) - len(ending)]
    if not surface.startswith(stem):
        return None
    tail = surface[len(stem) :]
    for candidate in CFORM_MAP.get(cform, ()):
        if tail in table.get(candidate, ()):
            return candidate
    return None


# ---------------------------------------------------------------------------
# 変換
# ---------------------------------------------------------------------------


def lexeme_key(entry, headword):
    """活用語の語彙素キー (辞書形の見出し, 品詞6つ組, 読み)."""
    ref = entry["dictionary"]
    if ref:
        return split_reference(ref)
    pos = tuple(entry[f"pos{i}"] for i in range(1, 7))
    return headword, pos, unescape(entry["reading"])


def collect_lexemes(paths):
    """活用語の語彙素ごとに、Sudachi が持つ活用形の集合を集める (IPAdic 活用型の判定用)."""
    forms = collections.defaultdict(set)
    for path in paths:
        for entry in read_lexicon(path):
            if entry["pos5"] == "*" or entry["left"] == "-1":
                continue
            headword = unescape(entry["headword"]) or unescape(entry["index"])
            key = lexeme_key(entry, headword)
            if key is not None:
                forms[key].add(entry["pos6"])
    return forms


# 既存辞書との重複判定のキー (表層形, 品詞大分類, 読み) の取り出し方
DEDUP_KEYS = {
    "surface": lambda surface, pos1, reading: surface,
    "reading": lambda surface, pos1, reading: (surface, reading),
    "pos-reading": lambda surface, pos1, reading: (surface, pos1, reading),
}


def load_existing(paths, key):
    """既存辞書 (MeCab 形式 CSV) のエントリを key で写した集合."""
    keys = set()
    for path in paths:
        for f in csv_files(path):
            # IPAdic は EUC-JP、NEologd 等は UTF-8。途中で失敗したら読み直す
            for encoding in ("utf-8", "euc_jp"):
                try:
                    with open(f, encoding=encoding, newline="") as fh:
                        for row in csv.reader(fh):
                            if len(row) >= 12:
                                keys.add(key(row[0], row[4], row[11]))
                    break
                except UnicodeDecodeError:
                    continue
    return keys


def convert(args):
    """Raw CSV を読み、変換済みエントリの dict と統計を返す."""
    ids = ContextIds(args.ipadic_dir)
    groups = SCOPES[args.scope]
    lexemes = collect_lexemes(args.lex)
    dedup_key = DEDUP_KEYS[args.dedup_key]
    existing = load_existing(args.exclude_existing, dedup_key)
    stats = collections.Counter()
    skipped_log = None
    if args.skipped_log:
        Path(args.skipped_log).parent.mkdir(parents=True, exist_ok=True)
        skipped_log = open(args.skipped_log, "w", encoding="utf-8")
    out = {}

    def skip(reason, entry, surface=""):
        stats[f"skip:{reason}"] += 1
        if skipped_log:
            pos = ",".join(entry[f"pos{i}"] for i in range(1, 7))
            skipped_log.write(f"{reason}\t{surface or entry['index']}\t{pos}\n")

    for path in args.lex:
        for entry in read_lexicon(path):
            stats["rows"] += 1
            if entry["left"] == "-1" or entry["right"] == "-1":
                skip("split_only", entry)
                continue
            pos = tuple(entry[f"pos{i}"] for i in range(1, 7))
            mapped = map_pos(pos)
            if mapped is None:
                skip("pos_unmapped:" + pos[0], entry)
                continue
            group, ipos = mapped
            if group not in groups:
                skip("out_of_scope", entry)
                continue
            surface = unescape(entry["headword"]) or unescape(entry["index"])
            reading = unescape(entry["reading"])
            if group == "symbol" and reading in ("", "キゴウ"):
                reading = surface
            if pos[4] == "*":
                ctype = cform = "*"
                base = surface
            else:
                key = lexeme_key(entry, surface)
                if key is None:
                    skip("bad_dictionary_form", entry, surface)
                    continue
                base, base_pos, base_reading = key
                ctype = ipadic_ctype(
                    pos[0], pos[4], base, base_reading, lexemes.get(key, set())
                )
                if ctype is None:
                    skip("ctype_unmapped:" + pos[4], entry, surface)
                    continue
                cform = ipadic_cform(ctype, pos[5], surface, base)
                if cform is None:
                    skip("cform_unmapped:" + pos[5], entry, surface)
                    continue
            cid = ids.lookup(ipos, ctype, cform, base)
            if cid is None:
                skip("no_context_id", entry, surface)
                continue
            if dedup_key(surface, ipos[0], reading) in existing:
                skip("exists", entry, surface)
                continue
            cost = round(int(entry["cost"]) * args.cost_scale + args.cost_offset)
            cost = max(-32768, min(32767, cost))
            key = (surface, cid, ipos, ctype, cform, base, reading)
            prev = out.get(key)
            if prev is not None:
                stats["merged_duplicate"] += 1
                if cost >= prev:
                    continue
            else:
                stats[f"out:{group}"] += 1
            out[key] = cost
    if skipped_log:
        skipped_log.close()
    return out, stats


def write_csv(entries, path):
    """MeCab 形式 13 列で書き出す."""
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="") as f:
        writer = csv.writer(f, lineterminator="\n")
        for (surface, cid, ipos, ctype, cform, base, reading), cost in entries.items():
            writer.writerow(
                [surface, cid, cid, cost, *ipos, ctype, cform, base, reading, reading]
            )


def main():
    """SudachiDict raw CSV を hasami 用の MeCab 形式 CSV に変換する."""
    parser = argparse.ArgumentParser(
        description="SudachiDict raw (V1) CSV を IPAdic 互換の MeCab 形式 CSV に変換"
    )
    parser.add_argument(
        "--lex",
        action="append",
        required=True,
        help="small_lex.csv / core_lex.csv / notcore_lex.csv (複数指定可)",
    )
    parser.add_argument(
        "--ipadic-dir",
        required=True,
        help="left-id.def, right-id.def, rewrite.def を含む IPAdic ソース",
    )
    parser.add_argument(
        "--exclude-existing",
        action="append",
        default=[],
        help="(表層形, 品詞大分類, 読み) が同じ語を落とす既存辞書の CSV/ディレクトリ",
    )
    parser.add_argument(
        "--dedup-key",
        choices=sorted(DEDUP_KEYS),
        default="surface",
        help="既存辞書と重複とみなす単位 (surface: 表層形が既にあれば落とす, "
        "reading: 表層形+読み, pos-reading: 表層形+品詞大分類+読み。既定: surface)",
    )
    parser.add_argument(
        "--scope",
        choices=sorted(SCOPES),
        default="noun",
        help="取り込む品詞 (all / content / noun / proper, 既定: noun)",
    )
    parser.add_argument("--cost-scale", type=float, default=1.0)
    parser.add_argument("--cost-offset", type=float, default=0.0)
    parser.add_argument("--output", required=True, help="出力 CSV")
    parser.add_argument("--skipped-log", help="取り込まなかった行の理由を TSV で書く")
    args = parser.parse_args()

    start = time.time()
    entries, stats = convert(args)
    write_csv(entries, args.output)
    for key in sorted(stats):
        print(f"{key}\t{stats[key]}", file=sys.stderr)
    print(
        f"wrote {len(entries)} entries to {args.output}"
        f" in {time.time() - start:.1f}s",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
