#!/usr/bin/env python3
"""非公開辞書の参照コミットと、ビルドに使う checkout を検証する。"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def git(*args: str, directory: Path | None = None) -> str:
    """指定した checkout で Git を実行する。"""
    return subprocess.check_output(
        ["git", "-C", str(directory or ROOT), *args], text=True, encoding="utf-8"
    ).strip()


def settings() -> dict[str, str]:
    """取得するリポジトリと固定した版を読み、値を検証する。"""
    values = json.loads((ROOT / "scripts" / "dictionary-inputs.json").read_text())
    repository = values.get("repository", "")
    if (
        not isinstance(repository, str)
        or re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository) is None
    ):
        raise ValueError("独自辞書の repository は owner/name で指定してください")
    commit = values.get("commit", "")
    if not isinstance(commit, str) or re.fullmatch(r"[0-9a-f]{40}", commit) is None:
        raise ValueError("dict の参照コミットが不正です")
    return {"repository": repository, "commit": commit}


def revision() -> str:
    """dictionary-inputs.json に記録した固定版を返す。"""
    return settings()["commit"]


def verify(*, clean: bool = False) -> str:
    """入力の存在と固定版との一致を確認し、その版を返す。"""
    expected = revision()
    directory = ROOT / "dict"
    if not (directory / ".git").exists():
        raise ValueError(
            "独自辞書が未取得です。アクセス権のあるアカウントで "
            "dictionary-inputs.json に記録したリポジトリとコミットを dict に取得してください"
        )
    if git("rev-parse", "--show-prefix", directory=directory):
        raise ValueError("dict が独立した Git リポジトリではありません")
    if git("rev-parse", "HEAD", directory=directory) != expected:
        raise ValueError(
            "dict の checkout が hasami に記録された参照コミットと異なります"
        )
    for name in ["user", "user-remove", "foreign-names"]:
        if not any((directory / name).glob("*.csv")):
            raise ValueError(f"独自辞書の入力がありません: dict/{name}")
    if clean and git("status", "--porcelain", directory=directory):
        raise ValueError("dict の入力に未コミットの変更があります")
    return expected


def main() -> int:
    """CLI を実行し、検証できなければ非ゼロで終了する。"""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["revision", "repository", "verify"])
    parser.add_argument("--clean", action="store_true")
    args = parser.parse_args()
    try:
        if args.command == "repository":
            value = settings()["repository"]
        elif args.command == "revision":
            value = revision()
        else:
            value = verify(clean=args.clean)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"dictionary inputs: {error}", file=sys.stderr)
        return 1
    print(value)
    return 0


if __name__ == "__main__":
    sys.exit(main())
