r"""辞書から外国人名のエントリを探し、`hasami repair --remove` に渡す削除リストを作る.

音声合成の読み付けでは、中国・朝鮮系の 1 文字姓 (何=ガ、林=リン、金=キム) が日常語と
衝突して誤読を招く (「何なのか」→「ガナノカ」)。このスクリプトは人名エントリ
(名詞,固有名詞,人名,*) の読みを Unicode Unihan の字音と照合し、日本の姓名でないものを
削除候補として挙げる。

判定規則は候補を挙げるためのもので、最終的な判定は許可リスト (日本人名として残す) と
拒否リスト (規則で拾えない外国人名) で確定させる。規則の出力を変えたときは --audit の
差分を見て、誤判定を許可・拒否リストに足すこと。

- foreign-reading: 全漢字が朝鮮語の字音か普通話で読まれ、日本語の字音 (音読み・訓読み) では
  説明できない (金=キム、王=ワン、在訓=ジェフン)。由美=ユミ のように朝鮮語の字音とも
  一致するが日本語でも読めるものは挙げない
- cn-surname-on: 1 文字姓の音読み (林=リン、王=オウ、毛=モウ)。日本の姓として使われる
  もの (伴=バン、菅=カン) は許可リストで残す
- cn-compound-surname: 中国の複姓の音読み (司馬=シバ、諸葛=ショカツ)
- kata-foreign: カタカナの姓・名で、日本人名 (漢字・ひらがな表記) の読みに無いもの
- full-name: 外国の姓と名からなるフルネーム (毛沢東=モウタクトウ)、または全体が
  外国語読みのフルネーム (金正日=キムジョンイル)。--full に分けて出す

出力:
    --parts  姓・名と 1 文字の 人名,一般 の削除リスト (dict/user-remove/foreign-names.csv)
    --full   フルネームの削除リスト (任意で適用する。dict/foreign-names/full-names.csv)
    --audit  全候補と判定理由の TSV (レビュー用。許可リストで残したものも含む)

Usage:
    hasami export --dict dict/ipadic-neologd-sudachi.hsd --output /tmp/lex.csv
    python3 scripts/find_foreign_names.py /tmp/lex.csv \
        --parts dict/user-remove/foreign-names.csv \
        --full dict/foreign-names/full-names.csv \
        --audit /tmp/foreign-names-audit.tsv

入力は MeCab 形式の CSV (13 列。UTF-8 か EUC-JP) のファイルまたはディレクトリ。
Unihan は初回に --unihan のパスへダウンロードし、SHA-256 を検証する。
"""

import argparse
import collections
import csv
import functools
import hashlib
import io
import re
import sys
import unicodedata
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path

RULES_VERSION = "1"
UNIHAN_VERSION = "18.0.0"
UNIHAN_URL = f"https://www.unicode.org/Public/{UNIHAN_VERSION}/ucd/Unihan.zip"
UNIHAN_SHA256 = "4c93ea9c1f636451729a840978f1667a53886af37ba854fdcce109721c63d43e"
DEFAULT_UNIHAN = Path(".dict-src/unihan") / f"Unihan-{UNIHAN_VERSION}.zip"
DEFAULT_ALLOW = Path("dict/foreign-names/allow.csv")
DEFAULT_DENY = Path("dict/foreign-names/deny.csv")

NAME_POS = "名詞,固有名詞,人名"

# 中国の複姓 (百家姓ほか)。日本語の音読みで読まれているものだけを挙げる
# 漢字の一覧は空白区切りの方が読みやすいので str.split で書く
COMPOUND_SURNAMES = frozenset(
    """
    司馬 諸葛 欧陽 歐陽 上官 夏侯 皇甫 公孫 東方 令狐 長孫 慕容 宇文 尉遅 独孤 獨孤 南宮
    西門 鮮于 司徒 司空 端木 軒轅 呼延 万俟 聞人 赫連 公羊 澹台 公冶 宗政 濮陽 淳于 単于
    太叔 申屠 仲孫 鍾離 閭丘 子車 顓孫 巫馬 公西 漆雕 楽正 公良 拓跋 夾谷 宰父 穀梁 段干
    百里 東郭 南門 羊舌 微生 梁丘 左丘 東門 第五 耶律 完顔 愛新覚羅 賀蘭 乞伏 禿髪 爾朱
    斛律 叱干 是婁
    """.split()  # noqa: SIM905
)

# フルネームの分解に使う主要な中国・朝鮮の姓 (人口上位)。1 文字姓の削除候補には
# 珍しい字の姓エントリも混じるため、フルネームを外国人名とみなすのはこの姓に限る
COMMON_SURNAMES = frozenset(
    "王李張劉陳楊黄黃趙呉吳周徐孫馬朱胡郭何林高羅鄭梁謝宋唐許韓馮鄧曹彭曾曽蕭田董潘袁蔡蒋蔣"
    "余于杜葉程魏蘇呂丁任盧姚沈鍾姜崔譚陸范汪廖石金韋賈夏傅方鄒熊白孟秦邱侯江尹薛閻段雷龍竜"
    "黎史陶賀毛郝顧龔邵万萬覃武銭錢戴厳嚴莫孔向常湯康易喬頼賴文朴申権權安柳洪全裵裴南河成車"
    "禹具羅辛閔池元千玄咸卞廉辺邊秋魯都慎宣吉延房表明奇琴玉印諸卓智鞠殷片芮景奉夫"
)


# 2 文字のフルネーム (姓 1 字 + 名 1 字) を外国人名とみなす姓。2 文字の人名エントリには
# 日本の天皇・僧の名 (安徳、南渓) や名だけの登録 (竜童) が多いので、最頻出の姓に限る
TOP_SURNAMES = frozenset(
    "王李張劉陳楊黄趙呉周徐孫馬朱胡郭何林高羅鄭梁謝宋唐許韓馮鄧曹彭曾曽蕭董袁蔡蔣蒋魏呂盧崔金朴"
)

# 日本の称号・法号の語尾。これで終わる人名 (常高院、高峰顕日禅師、天秀尼) はフルネームの
# 規則から外す
JAPANESE_TITLE_SUFFIXES = (
    "院",
    "天皇",
    "皇后",
    "上人",
    "禅師",
    "法師",
    "大師",
    "和尚",
    "尼",
    "局",
    "丸",
    "姫",
    "御前",
    "太夫",
)


# ---------------------------------------------------------------- 仮名


_SMALL_TO_LARGE = str.maketrans("ァィゥェォャュョッヮヵヶ", "アイウエオヤユヨツワカケ")
_OLD_KANA = str.maketrans("ヲヱヰヂヅ", "オエイジズ")
_DAKU = str.maketrans(
    "カキクケコサシスセソタチツテトハヒフヘホ",
    "ガギグゲゴザジズゼゾダヂヅデドバビブベボ",
)
_HANDAKU = str.maketrans("ハヒフヘホ", "パピプペポ")
_VOWEL_ROWS = [
    ("アカサタナハマヤラワガザダバパャ", "ア"),
    ("イキシチニヒミリギジヂビピ", "イ"),
    ("ウクスツヌフムユルグズヅブプュ", "ウ"),
    ("エケセテネヘメレゲゼデベペ", "エ"),
    ("オコソトノホモヨロヲゴゾドボポョ", "オ"),
]
_VOWEL_OF = {c: v for row, v in _VOWEL_ROWS for c in row}


def to_kata(s):
    return "".join(chr(ord(c) + 0x60) if "ぁ" <= c <= "ゖ" else c for c in s)


def jnorm(s):
    """日本語の字音と照合するための正規化。

    旧式の大書き (ジユン = ジュン) と旧仮名 (サダヲ = サダオ) を吸収する。
    長音符は残す (日本語の字音に長音符は現れない)。
    """
    return s.translate(_SMALL_TO_LARGE).translate(_OLD_KANA)


def fnorm(s):
    """外国語の字音と照合するための正規化。長音符と中黒も落とす"""
    return jnorm(s).replace("ー", "").replace("・", "")


def long_vowel_variants(reading):
    """長音符を 母音・ウ・イ に戻した読みの候補 (ヨーコ → ヨオコ / ヨウコ)"""
    outs = [""]
    for i, ch in enumerate(reading):
        if ch == "ー" and i > 0:
            v = _VOWEL_OF.get(reading[i - 1], "ウ")
            alts = {v, "ウ" if v in "オウ" else v, "イ" if v in "エイ" else v}
            outs = [o + a for o in outs for a in alts]
        else:
            outs = [o + ch for o in outs]
    return outs


def is_kanji(c):
    o = ord(c)
    return (
        0x4E00 <= o <= 0x9FFF
        or 0x3400 <= o <= 0x4DBF
        or 0x20000 <= o <= 0x3FFFF
        or 0xF900 <= o <= 0xFAFF
        or c in "々〆"
    )


def is_kata(c):
    return "゠" <= c <= "ヿ"


def script(s):
    if s and all(map(is_kanji, s)):
        return "kanji"
    if s and all(map(is_kata, s)):
        return "kata"
    return "other"


# ---------------------------------------------------------------- Unihan


def ensure_unihan(path):
    """Unihan.zip が無ければダウンロードし、SHA-256 を検証する"""
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        print(f"Downloading {UNIHAN_URL} -> {path}", file=sys.stderr)
        with urllib.request.urlopen(UNIHAN_URL, timeout=120) as resp:
            data = resp.read()
        path.write_bytes(data)
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != UNIHAN_SHA256:
        raise SystemExit(
            f"{path}: SHA-256 mismatch (expected {UNIHAN_SHA256}, got {digest}). "
            "Delete the file and rerun to download it again."
        )
    return path


@dataclass
class Unihan:
    on: dict  # 漢字 → 音読み (カタカナ) の集合
    kun: dict  # 漢字 → 訓読み (カタカナ) の集合
    mandarin: dict  # 漢字 → 拼音 (声調なし) の集合
    hangul: dict  # 漢字 → 朝鮮語の字音 (ハングル 1 音節) の集合


def load_unihan(path):
    on, kun, mandarin, hangul = (collections.defaultdict(set) for _ in range(4))
    with zipfile.ZipFile(path) as zf, zf.open("Unihan_Readings.txt") as raw:
        for line in io.TextIOWrapper(raw, encoding="utf-8"):
            if line.startswith("#") or not line.strip():
                continue
            cp, field, value = line.rstrip("\n").split("\t", 2)
            ch = chr(int(cp.removeprefix("U+"), 16))
            if field == "kJapanese":
                for r in value.split():
                    r = r.replace("-", "").replace(".", "")
                    if r:
                        (on if is_kata(r[0]) else kun)[ch].add(to_kata(r))
            elif field in ("kJapaneseOn", "kJapaneseKun"):
                for r in value.split():
                    try:
                        k = roma_to_kata(_hepburn(r.lower()))
                    except ValueError:
                        continue
                    (on if field == "kJapaneseOn" else kun)[ch].add(k)
            elif field == "kMandarin":
                mandarin[ch].update(_strip_tone(p) for p in value.split())
            elif field in ("kHanyuPinyin", "kXHC1983"):
                for part in value.split():
                    mandarin[ch].update(
                        _strip_tone(p) for p in part.split(":", 1)[1].split(",")
                    )
            elif field == "kHangul":
                hangul[ch].update(h.split(":", 1)[0] for h in value.split())
    return Unihan(dict(on), dict(kun), dict(mandarin), dict(hangul))


def _strip_tone(p):
    p = unicodedata.normalize("NFD", p)
    p = "".join(c for c in p if unicodedata.category(c) != "Mn" or c == "̈")
    return unicodedata.normalize("NFC", p).replace("v", "ü")


def _hepburn(r):
    """Unihan の kJapaneseOn/Kun (ヘボン式) を roma_to_kata が読める形に。促音は q"""
    r = r.replace("tch", "qch")
    return re.sub(r"([kgsztdhbpfmr])\1", r"q\1", r)


# ---------------------------------------------------------------- ローマ字 → カタカナ

_ROMA = {}
for _cons, _row in [
    ("", "アイウエオ"),
    ("k", "カキクケコ"),
    ("g", "ガギグゲゴ"),
    ("s", ["サ", "スィ", "ス", "セ", "ソ"]),
    ("z", ["ザ", "ズィ", "ズ", "ゼ", "ゾ"]),
    ("t", ["タ", "ティ", "トゥ", "テ", "ト"]),
    ("d", ["ダ", "ディ", "ドゥ", "デ", "ド"]),
    ("n", "ナニヌネノ"),
    ("h", "ハヒフヘホ"),
    ("b", "バビブベボ"),
    ("p", "パピプペポ"),
    ("m", "マミムメモ"),
    ("y", ["ヤ", "イ", "ユ", "イェ", "ヨ"]),
    ("r", "ラリルレロ"),
    ("w", ["ワ", "ウィ", "ウ", "ウェ", "ウォ"]),
    ("f", ["ファ", "フィ", "フ", "フェ", "フォ"]),
    ("sh", ["シャ", "シ", "シュ", "シェ", "ショ"]),
    ("j", ["ジャ", "ジ", "ジュ", "ジェ", "ジョ"]),
    ("ch", ["チャ", "チ", "チュ", "チェ", "チョ"]),
    ("ts", ["ツァ", "ツィ", "ツ", "ツェ", "ツォ"]),
]:
    for _v, _k in zip("aiueo", _row):
        _ROMA[_cons + _v] = _k
for _cons, _base in [
    ("ky", "キ"),
    ("gy", "ギ"),
    ("ny", "ニ"),
    ("hy", "ヒ"),
    ("by", "ビ"),
    ("py", "ピ"),
    ("my", "ミ"),
    ("ry", "リ"),
]:
    for _v, _small in zip("auo", "ャュョ"):
        _ROMA[_cons + _v] = _base + _small
_ROMA.update({"kwa": "クァ", "gwa": "グァ", "kwo": "クォ", "gwo": "グォ"})
_ROMA.update({"hwa": "ファ", "hwe": "フェ", "hwi": "フィ", "hwo": "フォ"})


def roma_to_kata(s):
    """ローマ字 (母音・子音+母音・撥音 n・末子音・長音 `:`・促音 q) をカタカナに"""
    out, i = [], 0
    while i < len(s):
        c = s[i]
        if c == ":":
            out.append("ー")
            i += 1
            continue
        if c == "q":
            out.append("ッ")
            i += 1
            continue
        for n in (3, 2, 1):
            if s[i : i + n] in _ROMA:
                out.append(_ROMA[s[i : i + n]])
                i += n
                break
        else:
            # 撥音と朝鮮語の閉音節の末子音
            final = {
                "n": "ン",
                "m": "ム",
                "k": "ク",
                "p": "プ",
                "t": "ッ",
                "l": "ル",
            }.get(c)
            if final is None:
                raise ValueError(f"unconvertible roman: {s!r} at {i}")
            out.append(final)
            i += 1
    return "".join(out)


def _join(cons, vowel):
    """子音 + 母音列をつなぐ。シ・チ・ジ の拗音は y を落とす (sh + ya → sha)"""
    if cons in ("sh", "ch", "j"):
        if vowel.startswith("y"):
            vowel = vowel[1:] or "i"
        elif vowel.startswith("i") and len(vowel) > 1 and vowel[1] in "aou":
            vowel = vowel[1:]
    return cons + vowel


# ---------------------------------------------------------------- 普通話 (拼音)

_PY_INITIALS = [
    "zh", "ch", "sh", "b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h",
    "j", "q", "x", "r", "z", "c", "s", "y", "w",
]  # fmt: skip
# 声母 → カナの子音 (無気音は清音と濁音の両方で書かれる)
_PY_CONS = {
    "b": ["b", "p"], "p": ["p"], "m": ["m"], "f": ["f"], "d": ["d", "t"], "t": ["t"],
    "n": ["n"], "l": ["r"], "g": ["g", "k"], "k": ["k"], "h": ["h", "f"], "j": ["j", "ch"],
    "q": ["ch"], "x": ["sh"], "zh": ["j", "ch"], "ch": ["ch"], "sh": ["sh"], "r": ["r", "j"],
    "z": ["z", "ts"], "c": ["ts"], "s": ["s"], "": [""],
}  # fmt: skip
# 韻母 → 母音の書き方の候補
_PY_FINALS = {
    "a": ["a", "a:"], "o": ["o", "o:", "uo"], "e": ["o", "o:", "a", "a:", "u:", "e"],
    "i": ["i", "i:"], "u": ["u", "u:"], "ü": ["yu", "yu:", "yui"], "ai": ["ai"],
    "ei": ["ei", "e:", "e"], "ao": ["ao", "o:", "ou"], "ou": ["ou", "o:", "o"],
    "an": ["an", "en"], "en": ["en", "on", "in"], "ang": ["an"], "eng": ["on", "en", "un"],
    "ong": ["on", "un"], "er": ["aru", "a:ru", "ru"], "ia": ["ia", "ya"],
    "ie": ["ie", "ye", "e"], "iao": ["iao", "yao", "yo:", "yo"], "iu": ["iu", "yu:", "yu", "iou"],
    "ian": ["ien", "ian", "en", "yen"], "in": ["in"], "iang": ["ian", "yan"],
    "ing": ["in", "en"], "iong": ["yon", "yun", "ion"], "ua": ["ua", "wa"],
    "uo": ["uo", "o", "wo", "o:"], "uai": ["uai", "wai"], "ui": ["ui", "uei", "wei", "ei"],
    "uan": ["uan", "wan", "oan", "an"], "un": ["un", "uen", "on"],
    "uang": ["uan", "wan", "oan", "an"], "üe": ["yue", "ue"], "üan": ["yuan", "yuen", "en"],
    "ün": ["yun", "in"],
}  # fmt: skip
# y・w で始まる音節は韻母だけの音節として読む
_PY_YW = {
    "yi": "i", "ya": "ia", "ye": "ie", "yao": "iao", "you": "iu", "yan": "ian", "yin": "in",
    "yang": "iang", "ying": "ing", "yong": "iong", "yu": "ü", "yue": "üe", "yuan": "üan",
    "yun": "ün", "wu": "u", "wa": "ua", "wo": "uo", "wai": "uai", "wei": "ui", "wan": "uan",
    "wen": "un", "wang": "uang", "weng": "ong",
}  # fmt: skip


@functools.cache
def pinyin_kata(py):
    """拼音 1 音節 → カタカナ表記の候補"""
    if py in _PY_YW:
        ini, fin = "", _PY_YW[py]
    else:
        ini = next((i for i in _PY_INITIALS if py.startswith(i)), "")
        fin = py[len(ini) :]
        if ini in ("j", "q", "x") and fin.startswith("u"):
            fin = "ü" + fin[1:]
    if fin == "i" and ini in ("z", "c", "s", "zh", "ch", "sh", "r"):
        # 舌尖母音 (zi ci si zhi chi shi ri)
        vowels = ["u", "u:", "i", "i:"] if ini in ("z", "c", "s") else ["i", "i:", "u:"]
    else:
        vowels = _PY_FINALS.get(fin)
        if vowels is None:
            return frozenset()
    out = set()
    for c in _PY_CONS.get(ini, [ini]):
        for v in vowels:
            try:
                out.add(fnorm(roma_to_kata(_join(c, v))))
            except ValueError:
                pass
    return frozenset(out)


# ---------------------------------------------------------------- 朝鮮語 (ハングル)

_H_INI = ["k", "kk", "n", "t", "tt", "r", "m", "p", "pp", "s", "ss", "", "ch", "jj", "chh",
          "kh", "th", "ph", "h"]  # fmt: skip
# 初声 → カナの子音 (語中の有声化と、語頭の r → n の両方を許す)
_H_INI_CONS = {
    "k": ["k", "g"], "kk": ["k"], "n": ["n"], "t": ["t", "d"], "tt": ["t"], "r": ["r", "n"],
    "m": ["m"], "p": ["p", "b"], "pp": ["p"], "s": ["s"], "ss": ["s"], "": [""],
    "ch": ["ch", "j"], "jj": ["ch"], "chh": ["ch"], "kh": ["k"], "th": ["t"], "ph": ["p"],
    "h": ["h", ""],
}  # fmt: skip
_H_VOWELS = [["a"], ["e"], ["ya"], ["ye", "e"], ["o"], ["e"], ["yo"], ["ye", "e"], ["o"],
             ["wa"], ["we"], ["we", "e"], ["yo"], ["u"], ["wo"], ["we"], ["wi"], ["yu"], ["u"],
             ["ui", "i"], ["i"]]  # fmt: skip
# 終声 → 末子音
_H_FINALS = ["", "k", "k", "k", "n", "n", "n", "t", "l", "k", "m", "p", "l", "l", "p", "l", "m",
             "p", "p", "t", "t", "ng", "t", "t", "k", "t", "p", "t"]  # fmt: skip
_FINAL_KATA = {"k": ["ク", "ツ"], "n": ["ン"], "l": ["ル"], "m": ["ム", "ン"], "p": ["プ", "ツ"],
               "ng": ["ン"], "t": ["ツ", "ト"]}  # fmt: skip
# 連音化で次の音節の初声になる末子音
_FINAL_LIAISON = {"k": ["g", "k"], "n": ["n"], "l": ["r"], "m": ["m"], "p": ["b", "p"],
                  "t": ["d", "t"]}  # fmt: skip


def _hangul_parts(syllable):
    code = ord(syllable) - 0xAC00
    if not 0 <= code < 11172:
        return None
    return (
        _H_INI[code // 588],
        tuple(_H_VOWELS[(code % 588) // 28]),
        _H_FINALS[code % 28],
    )


@functools.cache
def _hangul_body(cons, vowels):
    out = set()
    for v in vowels:
        try:
            out.add(fnorm(roma_to_kata(_join(cons, v))))
        except ValueError:
            pass
    return frozenset(out)


# ---------------------------------------------------------------- 照合


def _match_mandarin(surface, target, uh):
    cands = []
    for ch in surface:
        forms = set()
        for py in uh.mandarin.get(ch, ()):
            forms |= pinyin_kata(py)
        if not forms:
            return False
        cands.append(forms)

    @functools.cache
    def go(i, j):
        if i == len(cands):
            return j == len(target)
        return any(target.startswith(f, j) and go(i + 1, j + len(f)) for f in cands[i])

    return go(0, 0)


def _match_korean(surface, target, uh):
    syllables = []
    for ch in surface:
        parts = [p for p in map(_hangul_parts, uh.hangul.get(ch, ())) if p]
        if not parts:
            return False
        syllables.append(parts)
    n = len(syllables)

    @functools.cache
    def go(i, j, carry):
        """i 文字目から読む。carry は連音化で前の音節から送られた子音"""
        if i == n:
            return j == len(target) and carry is None
        for ini, vowels, fin in syllables[i]:
            if carry is not None and ini not in ("", "h"):
                continue
            for cons in [carry] if carry is not None else _H_INI_CONS[ini]:
                for body in _hangul_body(cons, vowels):
                    if not target.startswith(body, j):
                        continue
                    k = j + len(body)
                    if not fin:
                        if go(i + 1, k, None):
                            return True
                        continue
                    for f in _FINAL_KATA[fin]:
                        if target.startswith(f, k) and go(i + 1, k + len(f), None):
                            return True
                    if i + 1 < n and any(
                        go(i + 1, k, c) for c in _FINAL_LIAISON.get(fin, ())
                    ):
                        return True
        return False

    return go(0, 0, None)


def foreign_reading(surface, reading, uh):
    """全文字が朝鮮語の字音か普通話で読まれているか ("korean" / "mandarin" / None)"""
    target = fnorm(reading)
    if _match_korean(surface, target, uh):
        return "korean"
    if _match_mandarin(surface, target, uh):
        return "mandarin"
    return None


def _on_forms(ch, uh):
    forms = set()
    for r in uh.on.get(ch, ()):
        r = jnorm(r)
        forms.add(r)
        if len(r) >= 2 and r[-1] in "ツチクキ":
            forms.add(r[:-1] + "ツ")  # 促音化
    return forms


def is_on_reading(surface, reading, uh):
    """読みが各文字の音読みの連結か (ン・ッ の後の半濁音化と促音化を許す)"""
    target = jnorm(reading)
    cands = [_on_forms(ch, uh) for ch in surface]
    if not all(cands):
        return False

    @functools.cache
    def go(i, j, after_n):
        if i == len(cands):
            return j == len(target)
        for f in cands[i]:
            for v in {f, f.translate(_HANDAKU)} if after_n else {f}:
                if target.startswith(v, j) and go(i + 1, j + len(v), v[-1] in "ンツ"):
                    return True
        return False

    return go(0, 0, False)


def _jp_forms(ch, uh):
    """1 文字の日本語読みの候補。名前でよく使う縮め方を含める

    - 音読みの長音を落とす (有 ユウ→ユ、礼 レイ→レ)
    - 音読みの末尾の ク・ツ・キ・チ を落とす (楽 ラク→ラ)
    - 訓読みの前部分 (麿 マロ→マ、百 モモ→モ)
    """
    forms = set()
    for r in uh.on.get(ch, ()):
        r = jnorm(r)
        forms.add(r)
        if len(r) >= 2 and r[-1] in "ツチクキ":
            forms.update((r[:-1] + "ツ", r[:-1]))
        if len(r) >= 2 and r[-1] in "ウイ":
            forms.add(r[:-1])
    for r in uh.kun.get(ch, ()):
        r = jnorm(r)
        forms.update(r[:n] for n in range(1, len(r) + 1))
    return forms


def japanese_readable(surface, reading, uh):
    """読みが各文字の日本語読みの連結で説明できるか (連濁・半濁音化を許す)"""
    target = jnorm(reading)
    cands = []
    for ch in surface:
        if ch == "々" and cands:
            cands.append(cands[-1])
            continue
        forms = _jp_forms(ch, uh)
        if not forms:
            return False
        cands.append(forms)

    @functools.cache
    def go(i, j):
        if i == len(cands):
            return j == len(target)
        for f in cands[i]:
            variants = (
                {f, jnorm(f.translate(_DAKU)), f.translate(_HANDAKU)} if i else {f}
            )
            if any(target.startswith(v, j) and go(i + 1, j + len(v)) for v in variants):
                return True
        return False

    return go(0, 0)


# ---------------------------------------------------------------- 辞書エントリ


@dataclass(frozen=True)
class Entry:
    surface: str
    reading: str
    pos: str

    @property
    def kind(self):
        """姓 / 名 / 一般"""
        return self.pos.split(",")[3]


def _decode(raw):
    for enc in ("utf-8", "euc_jp"):
        try:
            return raw.decode(enc)
        except UnicodeDecodeError:
            continue
    return raw.decode("utf-8", errors="replace")


def load_entries(paths):
    """MeCab 形式 CSV から人名エントリを重複なしで読む"""
    entries = set()
    files = []
    for p in map(Path, paths):
        files.extend(sorted(p.glob("*.csv")) if p.is_dir() else [p])
    for path in files:
        for row in csv.reader(io.StringIO(_decode(path.read_bytes()))):
            if len(row) < 13:
                continue
            pos = ",".join(row[4:8])
            if pos.startswith(NAME_POS + ",") and row[0] and row[11]:
                entries.add(Entry(row[0], row[11], pos))
    return entries, files


def load_list(path):
    """許可・拒否リスト (表層形,読み[,品詞接頭辞]) を読む"""
    items = []
    if not path.exists():
        return items
    text = _decode(path.read_bytes())
    rows = csv.reader(line for line in text.splitlines() if not line.startswith("#"))
    for row in rows:
        row = [c.strip() for c in row]
        if len(row) < 2 or not row[0]:
            continue
        prefix = tuple(row[2].split(",")) if len(row) >= 3 and row[2] else ()
        items.append((row[0], row[1], prefix))
    return items


def listed(entry, items_index):
    for prefix in items_index.get((entry.surface, jnorm(entry.reading)), ()):
        if tuple(entry.pos.split(",")[: len(prefix)]) == prefix:
            return True
    return False


def _index(items):
    idx = collections.defaultdict(list)
    for surface, reading, prefix in items:
        idx[(surface, jnorm(reading))].append(prefix)
    return idx


# ---------------------------------------------------------------- 判定


@dataclass
class Candidate:
    entry: Entry
    rule: str
    detail: str = ""


def surname_evidence(entries, uh):
    """1 文字姓 (音読み) ごとに、姓 + 実在する名 からなるフルネームを数える (参考情報)

    名が音読みでないもの (訓・名乗り) を日本式、音読み・外国語読みを中国式として数える。
    """
    given = {(e.surface, jnorm(e.reading)) for e in entries if e.kind == "名"}
    readings = collections.defaultdict(set)
    for e in entries:
        if (
            e.kind == "姓"
            and len(e.surface) == 1
            and is_on_reading(e.surface, e.reading, uh)
        ):
            readings[e.surface].add(jnorm(e.reading))
    ev = collections.defaultdict(lambda: [0, 0, []])
    for e in entries:
        s, rd = e.surface, jnorm(e.reading)
        if e.kind != "一般" or not 2 <= len(s) <= 4 or script(s) != "kanji":
            continue
        for sr in readings.get(s[0], ()):
            g, gr = s[1:], rd[len(sr) :]
            if not rd.startswith(sr) or not gr or (g, gr) not in given:
                continue
            e_ = ev[(s[0], sr)]
            chinese = is_on_reading(g, gr, uh) or foreign_reading(g, gr, uh)
            e_[1 if chinese else 0] += 1
            if len(e_[2]) < 3:
                e_[2].append(f"{s}({e.reading})")
    return ev


def classify_parts(entries, uh, allow, deny):
    """姓・名 (と 1 文字の 人名,一般) の候補を挙げる"""
    candidates = {}
    for e in entries:
        s, rd, kind = e.surface, e.reading, e.kind
        if script(s) != "kanji" or (kind == "一般" and len(s) != 1):
            continue
        rule = None
        f = foreign_reading(s, rd, uh) if kind != "姓" or len(s) == 1 else None
        if f and not japanese_readable(s, rd, uh):
            rule = f"foreign-reading:{f}"
        elif len(s) == 1 and kind in ("姓", "一般") and is_on_reading(s, rd, uh):
            rule = "cn-surname-on"
        elif kind == "姓" and s in COMPOUND_SURNAMES and is_on_reading(s, rd, uh):
            rule = "cn-compound-surname"
        if rule:
            candidates[e] = Candidate(e, rule)
    # カタカナの姓・名は、残る日本人名 (漢字・ひらがな表記) の読みと照合する
    japanese = collections.defaultdict(set)
    for e in entries:
        kept = e not in candidates or listed(e, allow)
        if e.kind in ("姓", "名") and script(e.surface) != "kata" and kept:
            japanese[e.kind].add(jnorm(e.reading))
    for e in entries:
        if e.kind not in ("姓", "名") or script(e.surface) != "kata":
            continue
        variants = long_vowel_variants(jnorm(e.reading))
        if not any(v in japanese[e.kind] for v in variants):
            candidates[e] = Candidate(e, "kata-foreign")
    for e in entries:
        is_part = e.kind != "一般" or len(e.surface) == 1
        if is_part and e not in candidates and listed(e, deny):
            candidates[e] = Candidate(e, "deny-list")
    return candidates


def classify_full_names(entries, uh, removed_parts, allow, deny):
    """外国人のフルネーム (2 文字以上の 人名,一般) の候補を挙げる

    - 全体が朝鮮語の字音か普通話で読まれ、日本語の字音では説明できないもの (金正日)
    - 主要な中国・朝鮮の姓 (削除対象の読み) + 音読みか外国語読みの名 (毛沢東、劉備)。
      名が訓読みのもの (孫正義、王貞治) と、2 文字以上の日本の姓で始まると読めるもの
      (伊藤亜美 を 伊 + 藤亜美 と分けない) は挙げない
    """
    surnames = collections.defaultdict(set)
    for e in removed_parts:
        if e.kind == "姓" and (
            e.surface in COMMON_SURNAMES or e.surface in COMPOUND_SURNAMES
        ):
            surnames[e.surface].add(jnorm(e.reading))
    japanese_surnames = collections.defaultdict(set)
    for e in entries:
        if e.kind == "姓" and len(e.surface) >= 2 and e not in removed_parts:
            japanese_surnames[e.surface].add(jnorm(e.reading))
    candidates = {}
    for e in entries:
        s, rd = e.surface, jnorm(e.reading)
        if e.kind != "一般" or len(s) < 2:
            continue
        if listed(e, allow):
            continue
        if listed(e, deny):
            candidates[e] = Candidate(e, "deny-list")
            continue
        if script(s) != "kanji":
            continue
        f = foreign_reading(s, e.reading, uh)
        if f and not japanese_readable(s, e.reading, uh):
            candidates[e] = Candidate(e, f"full-name:{f}")
            continue
        if len(s) > 4 or s.endswith(JAPANESE_TITLE_SUFFIXES):
            continue
        if len(s) == 2 and s[0] not in TOP_SURNAMES:
            continue
        if any(
            rd.startswith(r)
            for k in range(2, len(s))
            for r in japanese_surnames.get(s[:k], ())
        ):
            continue
        for k in (1, 2):
            for sr in surnames.get(s[:k], ()):
                g, gr = s[k:], rd[len(sr) :]
                if not g or not gr or not rd.startswith(sr):
                    continue
                if is_on_reading(g, gr, uh) or foreign_reading(g, gr, uh):
                    candidates[e] = Candidate(
                        e, "full-name:surname+given", f"{s[:k]}({sr})"
                    )
                    break
            if e in candidates:
                break
    return candidates


# ---------------------------------------------------------------- 出力


def _sort_key(e):
    return (e.pos, e.surface, e.reading)


def write_list(path, entries_rules, title, inputs):
    counts = collections.Counter(rule.split(":")[0] for rule in entries_rules.values())
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as f:
        f.write(f"# {title}\n")
        f.write("#\n")
        f.write("# scripts/find_foreign_names.py が生成する。手で編集せず、誤判定は\n")
        f.write(
            "# dict/foreign-names/allow.csv (残す) と deny.csv (消す) に書いて再生成する。\n"
        )
        f.write(f"# 規則の版: {RULES_VERSION} / Unihan: {UNIHAN_VERSION}\n")
        f.write(f"# 入力: {', '.join(p.name for p in inputs)}\n")
        f.write(
            f"# 件数: {len(entries_rules)} ({', '.join(f'{k} {v}' for k, v in sorted(counts.items()))})\n"
        )
        w = csv.writer(f, lineterminator="\n")
        for e in sorted(entries_rules, key=_sort_key):
            w.writerow([e.surface, e.reading, e.pos])


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument(
        "inputs", nargs="+", help="MeCab 形式 CSV のファイルまたはディレクトリ"
    )
    ap.add_argument("--parts", type=Path, help="姓・名の削除リストの出力先")
    ap.add_argument("--full", type=Path, help="フルネームの削除リストの出力先")
    ap.add_argument("--audit", type=Path, help="全候補と判定理由の TSV の出力先")
    ap.add_argument(
        "--allow", type=Path, default=DEFAULT_ALLOW, help="残す日本人名の一覧"
    )
    ap.add_argument(
        "--deny", type=Path, default=DEFAULT_DENY, help="規則で拾えない外国人名"
    )
    ap.add_argument(
        "--unihan", type=Path, default=DEFAULT_UNIHAN, help="Unihan.zip のパス"
    )
    args = ap.parse_args(argv)

    uh = load_unihan(ensure_unihan(args.unihan))
    entries, files = load_entries(args.inputs)
    allow = _index(load_list(args.allow))
    deny = _index(load_list(args.deny))
    print(
        f"Loaded {len(entries)} person-name entries from {len(files)} files",
        file=sys.stderr,
    )

    part_cands = classify_parts(entries, uh, allow, deny)
    removed_parts = {e: c.rule for e, c in part_cands.items() if not listed(e, allow)}
    full_cands = classify_full_names(entries, uh, removed_parts, allow, deny)
    removed_full = {e: c.rule for e, c in full_cands.items()}

    if args.parts:
        write_list(args.parts, removed_parts, "外国人名の削除リスト (姓・名)", files)
    if args.full:
        write_list(args.full, removed_full, "外国人名の削除リスト (フルネーム)", files)
    if args.audit:
        ev = surname_evidence(entries, uh)
        with args.audit.open("w", encoding="utf-8", newline="") as f:
            w = csv.writer(f, delimiter="\t", lineterminator="\n")
            w.writerow(["decision", "rule", "surface", "reading", "pos", "detail"])
            for c in sorted(
                [*part_cands.values(), *full_cands.values()],
                key=lambda c: _sort_key(c.entry),
            ):
                e = c.entry
                decision = "keep" if listed(e, allow) else "remove"
                detail = c.detail
                if c.rule == "cn-surname-on" and (e.surface, jnorm(e.reading)) in ev:
                    jp, cn, ex = ev[(e.surface, jnorm(e.reading))]
                    detail = f"jp={jp} cn={cn} {' '.join(ex)}"
                w.writerow([decision, c.rule, e.surface, e.reading, e.pos, detail])
    by_rule = collections.Counter(r.split(":")[0] for r in removed_parts.values())
    print(f"parts: {len(removed_parts)} {dict(by_rule)}", file=sys.stderr)
    by_rule = collections.Counter(r.split(":")[0] for r in removed_full.values())
    print(f"full names: {len(removed_full)} {dict(by_rule)}", file=sys.stderr)


if __name__ == "__main__":
    main()
