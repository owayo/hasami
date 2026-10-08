"""配布辞書の取得版を読み、公式の配布先から最新版と SHA-256 を記録する。"""

import argparse
import copy
import hashlib
import json
import re
import shlex
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
import zipfile
from datetime import date
from pathlib import Path
from urllib.parse import urlencode

IPADIC_REPO = "https://github.com/taku910/mecab.git"
NEOLOGD_REPO = "https://github.com/neologd/mecab-ipadic-neologd.git"
SUDACHI_BUCKET = "https://sudachi.s3.ap-northeast-1.amazonaws.com"
SUDACHI_PREFIX = "sudachidict-raw/v1/"
SUDACHI_FILES = ("small_lex.zip", "core_lex.zip")
SOURCES_PATH = Path(__file__).with_name("dictionary-sources.json")
S3_NS = {"s3": "http://s3.amazonaws.com/doc/2006-03-01/"}


def version_key(version):
    """日付・年月と任意の改訂番号を、桁数によらず新しい順に比較する。"""
    if not re.fullmatch(r"[0-9]{6}(?:[0-9]{2})?(?:\.[0-9]+)?", version):
        raise ValueError(f"Invalid Sudachi raw version: {version!r}")
    date_text, _, revision = version.partition(".")
    date(int(date_text[:4]), int(date_text[4:6]), int(date_text[6:] or "1"))
    return int(date_text.ljust(8, "0")), int(revision or "0")


def load_sources(path):
    sources = json.loads(path.read_text(encoding="utf-8"))
    for name in ("ipadic", "neologd"):
        if not re.fullmatch(r"[0-9a-f]{40}", sources[name]["commit"]):
            raise ValueError(f"Invalid {name} commit")
    version_key(sources["sudachi"]["version"])
    hashes = sources["sudachi"]["sha256"]
    if set(hashes) != set(SUDACHI_FILES):
        raise ValueError("Sudachi raw requires small_lex.zip and core_lex.zip")
    for name, digest in hashes.items():
        if not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise ValueError(f"Invalid SHA-256: {name}")
    return sources


def shell_settings(sources):
    """Bash が読む設定を、値をシェルのコードとして解釈させずに出力する。"""
    values = {
        "IPADIC_REPO": IPADIC_REPO,
        "IPADIC_COMMIT": sources["ipadic"]["commit"],
        "NEOLOGD_REPO": NEOLOGD_REPO,
        "NEOLOGD_COMMIT": sources["neologd"]["commit"],
        "SUDACHI_VERSION": sources["sudachi"]["version"],
        "SUDACHI_URL": f"{SUDACHI_BUCKET}/{SUDACHI_PREFIX}{sources['sudachi']['version']}",
    }
    lines = [f"{key}={shlex.quote(value)}" for key, value in values.items()]
    files = [
        shlex.quote(f"{name} {sources['sudachi']['sha256'][name]}")
        for name in SUDACHI_FILES
    ]
    lines.append(f"SUDACHI_FILES=({' '.join(files)})")
    return "\n".join(lines)


def latest_commit(repo):
    result = subprocess.run(
        ["git", "ls-remote", "--exit-code", repo, "HEAD"],
        check=True,
        capture_output=True,
        text=True,
        timeout=120,
    )
    rows = result.stdout.splitlines()
    if len(rows) != 1 or not re.fullmatch(r"[0-9a-f]{40}\s+HEAD", rows[0]):
        raise ValueError(f"Could not resolve HEAD: {repo}")
    return rows[0].split()[0]


def curl(url, destination=None):
    command = [
        "curl",
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--retry",
        "3",
        "--connect-timeout",
        "15",
        "--max-time",
        "300",
    ]
    if destination is not None:
        command.extend(["--output", str(destination)])
    result = subprocess.run(command + [url], check=True, capture_output=True)
    return result.stdout


def list_raw(prefix):
    """S3 の公開一覧を最後のページまで読む。ETag は SHA-256 として扱わない。"""
    params = {"list-type": "2", "prefix": prefix, "delimiter": "/"}
    directories, files, tokens = [], set(), set()
    while True:
        root = ET.fromstring(curl(f"{SUDACHI_BUCKET}/?{urlencode(params)}"))
        directories.extend(
            node.text
            for node in root.findall("s3:CommonPrefixes/s3:Prefix", S3_NS)
            if node.text
        )
        files.update(
            node.text for node in root.findall("s3:Contents/s3:Key", S3_NS) if node.text
        )
        truncated = root.findtext("s3:IsTruncated", namespaces=S3_NS)
        if truncated == "false":
            return directories, files
        token = root.findtext("s3:NextContinuationToken", namespaces=S3_NS)
        if truncated != "true" or not token or token in tokens:
            raise ValueError("Invalid or incomplete Sudachi raw listing")
        tokens.add(token)
        params["continuation-token"] = token


def latest_raw_version():
    directories, _ = list_raw(SUDACHI_PREFIX)
    versions = set()
    for directory in directories:
        if not directory.startswith(SUDACHI_PREFIX) or not directory.endswith("/"):
            continue
        version = directory[len(SUDACHI_PREFIX) : -1]
        try:
            version_key(version)
        except ValueError:
            continue
        versions.add(version)
    for version in sorted(versions, key=version_key, reverse=True):
        prefix = f"{SUDACHI_PREFIX}{version}/"
        _, files = list_raw(prefix)
        if all(f"{prefix}{name}" in files for name in SUDACHI_FILES):
            return version
        print(f"Skip incomplete Sudachi raw: {version}")
    raise ValueError("No complete Sudachi raw dictionary is published")


def download_raw(version, name, directory):
    """最新の実体を取り、CSV と ZIP の整合性を確かめてハッシュを計算する。"""
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / name
    with tempfile.NamedTemporaryFile(dir=directory, delete=False) as file:
        temporary = Path(file.name)
    try:
        curl(f"{SUDACHI_BUCKET}/{SUDACHI_PREFIX}{version}/{name}", temporary)
        with zipfile.ZipFile(temporary) as archive:
            # build-dict.sh が unzip するので、予期した CSV だけを受け入れる。
            if archive.namelist() != [name.replace(".zip", ".csv")]:
                raise ValueError(f"Unexpected ZIP contents: {name}")
            if archive.testzip() is not None:
                raise ValueError(f"Corrupt ZIP: {name}")
        with temporary.open("rb") as file:
            digest = hashlib.file_digest(file, "sha256").hexdigest()
        temporary.replace(path)
        return digest
    finally:
        temporary.unlink(missing_ok=True)


def update_sources(path, src):
    original = load_sources(path)
    updated = copy.deepcopy(original)
    updated["ipadic"]["commit"] = latest_commit(IPADIC_REPO)
    updated["neologd"]["commit"] = latest_commit(NEOLOGD_REPO)
    version = latest_raw_version()
    if version_key(version) < version_key(original["sudachi"]["version"]):
        raise ValueError(
            "The latest published Sudachi raw is older than the recorded version"
        )
    updated["sudachi"] = {
        "version": version,
        "sha256": {
            name: download_raw(version, name, src / "sudachi-raw" / version)
            for name in SUDACHI_FILES
        },
    }
    # 同じ版の ZIP が差し替えられた場合も検出する。ビルド・検証後に Actions がコミットする。
    if updated == original:
        print("Dictionary sources are up to date")
        return False
    with tempfile.NamedTemporaryFile(
        mode="w", encoding="utf-8", dir=path.parent, delete=False
    ) as file:
        temporary = Path(file.name)
        json.dump(updated, file, ensure_ascii=False, indent=2)
        file.write("\n")
    try:
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)
    print(json.dumps(updated, ensure_ascii=False, indent=2))
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("shell", "update"))
    parser.add_argument("--sources", type=Path, default=SOURCES_PATH)
    parser.add_argument("--src", type=Path, default=Path(".dict-src"))
    args = parser.parse_args()
    try:
        if args.command == "shell":
            print(shell_settings(load_sources(args.sources)))
        else:
            update_sources(args.sources, args.src)
    except subprocess.CalledProcessError as error:
        if error.stderr:
            detail = (
                error.stderr.decode()
                if isinstance(error.stderr, bytes)
                else error.stderr
            )
            print(detail.rstrip(), file=sys.stderr)
        raise SystemExit(str(error)) from error
    except (OSError, ValueError, ET.ParseError, zipfile.BadZipFile) as error:
        raise SystemExit(str(error)) from error


if __name__ == "__main__":
    main()
