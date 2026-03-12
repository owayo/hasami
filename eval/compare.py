"""複数エンジン・辞書での形態素解析結果を比較する.

Usage:
    python eval/compare.py "二日酔い"
    python eval/compare.py "東京都に住んでいる" "鬼滅の刃が大人気"
    python eval/compare.py --file sentences.txt
"""

import argparse
import os
import subprocess
import sys
from pathlib import Path

# プロジェクトルート
PROJECT_ROOT = Path(__file__).resolve().parent.parent

# hasami 辞書設定
HASAMI_DICTS = [
    ("hasami (ipadic)", "dict/ipadic.hsd"),
    ("hasami (neologd)", "dict/ipadic-neologd.hsd"),
    ("hasami (unified)", "dict/ipadic-neologd-sudachi.hsd"),
    ("hasami (sudachi)", "dict/sudachi.hsd"),
]

# sudachi.rs 設定
SUDACHI_RS_ROOT = os.environ.get("SUDACHI_RS_ROOT", r"C:\GitHub\sudachi.rs")
SUDACHI_RS_DICT = os.environ.get(
    "SUDACHI_RS_DICT",
    os.path.join(
        os.path.expanduser("~"),
        ".rye/py/cpython@3.12.9/Lib/site-packages/"
        "sudachidict_core/resources/system.dic",
    ),
)


def _find_hasami_binary():
    """Hasami バイナリを探す.

    Returns:
        str: hasami バイナリのパス。

    """
    release = PROJECT_ROOT / "target" / "release" / "hasami.exe"
    if release.exists():
        return str(release)
    release_no_ext = PROJECT_ROOT / "target" / "release" / "hasami"
    if release_no_ext.exists():
        return str(release_no_ext)
    return "hasami"


def _find_sudachi_binary():
    """sudachi.rs バイナリを探す.

    Returns:
        str or None: sudachi バイナリのパス。

    """
    root = Path(SUDACHI_RS_ROOT)
    for name in ["sudachi.exe", "sudachi"]:
        path = root / "target" / "release" / name
        if path.exists():
            return str(path)
    return None


def tokenize_hasami(text, dict_path):
    """Hasami で形態素解析する.

    Returns:
        str: 解析結果。

    """
    binary = _find_hasami_binary()
    full_path = PROJECT_ROOT / dict_path
    if not full_path.exists():
        return "(辞書なし)"

    try:
        result = subprocess.run(
            [binary, "tokenize", "--dict", str(full_path)],
            input=text,
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=10,
        )
        return result.stdout.rstrip("\n") if result.returncode == 0 else "(エラー)"
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return "(実行失敗)"


def tokenize_sudachi(text):
    """sudachi.rs で形態素解析する.

    Returns:
        str: 解析結果。

    """
    binary = _find_sudachi_binary()
    if not binary:
        return "(sudachi.rs 未検出)"
    if not Path(SUDACHI_RS_DICT).exists():
        return "(辞書未検出)"

    try:
        result = subprocess.run(
            [binary, "-l", SUDACHI_RS_DICT, "-a"],
            input=text,
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=10,
        )
        return result.stdout.rstrip("\n") if result.returncode == 0 else "(エラー)"
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return "(実行失敗)"


def extract_tokens(output, is_sudachi_rs=False):
    """解析出力から表層形と読みのリストを抽出する.

    Returns:
        list[tuple[str, str]]: (表層形, 読み) のリスト。

    """
    tokens = []
    for line in output.splitlines():
        if line == "EOS" or not line.strip():
            continue
        parts = line.split("\t")
        if not parts:
            continue
        surface = parts[0]
        reading = ""
        if is_sudachi_rs and len(parts) >= 4:
            # sudachi.rs -a: surface\tpos\tnorm\tsurf\treading
            reading = parts[4] if len(parts) >= 5 else ""
        elif len(parts) >= 2:
            # hasami format: surface \t pos,reading,pronunciation
            fields = parts[1].split(",")
            # hasami output: POS1,POS2,POS3,POS4,base_form,reading,pronunciation
            if len(fields) >= 6:
                reading = fields[5]
        tokens.append((surface, reading))
    return tokens


def compare(text):
    """1つのテキストを全エンジンで解析して比較表示する."""
    print(f"\n{'=' * 60}")
    print(f"  入力: {text}")
    print(f"{'=' * 60}")

    results = []

    # hasami 各辞書
    for label, dict_path in HASAMI_DICTS:
        output = tokenize_hasami(text, dict_path)
        tokens = extract_tokens(output)
        results.append((label, tokens, output))

    # sudachi.rs
    output = tokenize_sudachi(text)
    tokens = extract_tokens(output, is_sudachi_rs=True)
    results.append(("sudachi.rs", tokens, output))

    # 分割 + 読みサマリ
    print()
    max_label = max(len(r[0]) for r in results)
    for label, tokens, _ in results:
        if not tokens:
            print(f"  {label:<{max_label}}  (解析失敗)")
            continue
        seg = " | ".join(t[0] for t in tokens)
        reading = "".join(t[1] for t in tokens)
        print(f"  {label:<{max_label}}  {seg}")
        print(f"  {'':<{max_label}}  読み: {reading}")

    # 詳細出力
    print(f"\n{'─' * 60}")
    for label, _, output in results:
        if "(辞書なし)" in output or "(未検出)" in output or "(失敗)" in output:
            continue
        print(f"\n  [{label}]")
        for line in output.splitlines():
            if line.strip():
                print(f"    {line}")
    print()


def main():
    """複数エンジンで形態素解析結果を比較する."""
    parser = argparse.ArgumentParser(
        description="Compare tokenization across engines and dictionaries"
    )
    parser.add_argument("texts", nargs="*", help="テキスト（複数可）")
    parser.add_argument("--file", "-f", help="入力テキストファイル（1行1文）")
    args = parser.parse_args()

    texts = list(args.texts) if args.texts else []

    if args.file:
        with open(args.file, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if line:
                    texts.append(line)

    if not texts:
        print('Usage: python eval/compare.py "テキスト"', file=sys.stderr)
        print("       python eval/compare.py --file sentences.txt", file=sys.stderr)
        sys.exit(1)

    for text in texts:
        compare(text)


if __name__ == "__main__":
    main()
