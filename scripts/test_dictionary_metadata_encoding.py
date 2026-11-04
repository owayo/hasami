"""メタデータ用の説明を、PythonとBashの実プロセスで照合する。"""

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

PREPARE = Path(__file__).resolve().with_name("prepare_ipadic.py")


class DictionaryMetadataEncodingTests(unittest.TestCase):
    """環境の標準出力設定に左右されず日本語を渡せることを検査する。"""

    def setUp(self):
        """公開情報に依存しない最小の合成IPAdicを用意する。"""
        self.workspace = tempfile.TemporaryDirectory(prefix="hasami-metadata-")
        self.addCleanup(self.workspace.cleanup)
        self.root = Path(self.workspace.name)
        self.source = self.root / "source"
        self.source.mkdir()
        (self.source / "char.def").write_text(
            "SYMBOL 1 1 0\nHIRAGANA 0 1 2\nALPHA 1 1 0\nNUMERIC 1 1 0\n0x00D0 SPACE\n",
            encoding="utf-8",
        )
        (self.source / "unk.def").write_text(
            "SYMBOL,5,5,4769,名詞,サ変接続,*,*,*,*,*\n", encoding="utf-8"
        )
        (self.source / "matrix.def").write_text("1 1\n0 0 0\n", encoding="utf-8")
        (self.source / "synthetic.csv").write_bytes(
            "％,0,0,1,名詞,接尾,助数詞,*,*,*,％,パーセント,パーセント\n"
            "〜,0,0,1,記号,一般,*,*,*,*,〜,カラ,カラ\n".encode("euc_jp")
        )

    def check_description(self, result):
        """説明の生バイトをUTF-8で読み、日本語と一行の契約を照合する。

        Returns:
            照合した標準出力の生バイト。

        """
        self.assertEqual(result.returncode, 0, result.stderr)
        description = result.stdout.decode("utf-8", errors="strict")
        self.assertEqual(len(description.splitlines()), 1)
        self.assertIn("unk.def SYMBOL=記号,一般;", description)
        self.assertIn("1 CP932-side variants", description)
        self.assertIn("unit symbols as 名詞,接尾,助数詞", description)
        self.assertFalse(any(0x80 <= ord(c) <= 0x9F for c in description))
        return result.stdout

    def test_cli_description_is_utf8_regardless_of_inherited_stdout_encoding(self):
        """CP932・ASCII・UTF-8の環境から同じUTF-8の説明を得る。"""
        descriptions = []
        for encoding in ("cp932", "ascii", "utf-8"):
            with self.subTest(encoding=encoding):
                result = subprocess.run(
                    [
                        sys.executable,
                        str(PREPARE),
                        str(self.source),
                        str(self.root / encoding),
                    ],
                    env=dict(os.environ, PYTHONIOENCODING=encoding, PYTHONUTF8="0"),
                    capture_output=True,
                    check=False,
                )
                descriptions.append(self.check_description(result))
        self.assertTrue(all(item == descriptions[0] for item in descriptions))

    def test_bash_command_substitution_preserves_japanese_metadata(self):
        """ビルドと同じコマンド置換でも説明を文字化けさせない。"""
        bash = shutil.which("bash")
        self.assertIsNotNone(bash, "Git for Windows等のBashが必要")
        result = subprocess.run(
            [
                bash,
                "-euo",
                "pipefail",
                "-c",
                'patch=$("$1" "$2" "$3" "$4"); printf "%s\\n" "$patch"',
                "--",
                Path(sys.executable).as_posix(),
                PREPARE.as_posix(),
                self.source.as_posix(),
                (self.root / "bash-output").as_posix(),
            ],
            env=dict(os.environ, PYTHONIOENCODING="cp932", PYTHONUTF8="0"),
            capture_output=True,
            check=False,
        )
        self.check_description(result)


if __name__ == "__main__":
    unittest.main()
