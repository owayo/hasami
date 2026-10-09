#!/usr/bin/env python3
"""非公開辞書の取得ブランチと、ビルドに使う checkout を検証する。"""

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
    """取得するリポジトリとブランチを読み、値を検証する。"""
    values = json.loads(
        (ROOT / "scripts" / "dictionary-inputs.json").read_text(encoding="utf-8")
    )
    repository = values.get("repository", "")
    if (
        not isinstance(repository, str)
        or re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository) is None
    ):
        raise ValueError("独自辞書の repository は owner/name で指定してください")
    branch = values.get("branch", "")
    if not isinstance(branch, str) or not branch or branch.startswith("-"):
        raise ValueError("dict の取得ブランチが不正です")
    try:
        git("check-ref-format", f"refs/heads/{branch}")
    except subprocess.CalledProcessError as error:
        raise ValueError("dict の取得ブランチが不正です") from error
    return {"repository": repository, "branch": branch}


def ref() -> str:
    """辞書を取得するブランチの完全な参照名を返す。"""
    return f"refs/heads/{settings()['branch']}"


def verify(*, clean: bool = False) -> str:
    """入力の存在と状態を確認し、実際に使うコミットを返す。"""
    settings()
    directory = ROOT / "dict"
    if not (directory / ".git").exists():
        raise ValueError(
            "独自辞書が未取得です。アクセス権のあるアカウントで "
            "dictionary-inputs.json に記録したリポジトリのブランチを dict に取得してください"
        )
    if git("rev-parse", "--show-prefix", directory=directory):
        raise ValueError("dict が独立した Git リポジトリではありません")
    revision = git("rev-parse", "HEAD", directory=directory)
    for name in ["user", "user-remove", "foreign-names"]:
        if not any((directory / name).glob("*.csv")):
            raise ValueError(f"独自辞書の入力がありません: dict/{name}")
    if clean and git("status", "--porcelain", directory=directory):
        raise ValueError("dict の入力に未コミットの変更があります")
    return revision


def main() -> int:
    """CLI を実行し、検証できなければ非ゼロで終了する。"""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["ref", "repository", "verify"])
    parser.add_argument("--clean", action="store_true")
    args = parser.parse_args()
    try:
        if args.command == "repository":
            value = settings()["repository"]
        elif args.command == "ref":
            value = ref()
        else:
            value = verify(clean=args.clean)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"dictionary inputs: {error}", file=sys.stderr)
        return 1
    print(value)
    return 0


if __name__ == "__main__":
    sys.exit(main())
