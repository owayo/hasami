"""非公開辞書のブランチ指定と入力の検証を実際の Git で確かめる。"""

import csv
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import dictionary_inputs
import select_dictionary_words


class DictionaryInputsTests(unittest.TestCase):
    def setUp(self):
        self.workspace = tempfile.TemporaryDirectory()
        self.addCleanup(self.workspace.cleanup)
        self.root = Path(self.workspace.name)
        self.dictionary = self.root / "dict"
        self.dictionary.mkdir()
        for directory in [self.root, self.dictionary]:
            self.run_git(directory, "init", "--quiet")
            self.run_git(directory, "config", "user.name", "Dictionary test")
            self.run_git(directory, "config", "user.email", "test@example.invalid")
        for name in ["user", "user-remove", "foreign-names"]:
            folder = self.dictionary / name
            folder.mkdir()
            (folder / "synthetic.csv").write_text("synthetic\n", encoding="utf-8")
        self.commit_dictionary()
        self.expected = self.run_git(self.dictionary, "rev-parse", "HEAD")
        (self.root / "scripts").mkdir()
        self.settings = self.root / "scripts" / "dictionary-inputs.json"
        self.settings.write_text(
            json.dumps(
                {
                    "repository": "example/hasami-dictionaries",
                    "branch": "main",
                }
            )
        )
        root_patch = patch.object(dictionary_inputs, "ROOT", self.root)
        root_patch.start()
        self.addCleanup(root_patch.stop)

    def run_git(self, directory, *args):
        return subprocess.check_output(
            ["git", "-C", str(directory), *args], text=True, encoding="utf-8"
        ).strip()

    def commit_dictionary(self):
        self.run_git(self.dictionary, "add", ".")
        self.run_git(self.dictionary, "commit", "--quiet", "-m", "Synthetic inputs")

    def test_matching_checkout(self):
        self.assertEqual(dictionary_inputs.verify(clean=True), self.expected)

    def test_uninitialized_checkout_is_rejected(self):
        (self.dictionary / ".git").rename(self.root / "dictionary-git-backup")
        with self.assertRaisesRegex(ValueError, "未取得"):
            dictionary_inputs.verify()

    def test_advanced_head_is_used_without_updating_settings(self):
        original_settings = self.settings.read_bytes()
        (self.dictionary / "user" / "synthetic.csv").write_text("changed\n")
        self.commit_dictionary()
        current = self.run_git(self.dictionary, "rev-parse", "HEAD")
        self.assertNotEqual(current, self.expected)
        self.assertEqual(dictionary_inputs.verify(clean=True), current)
        self.assertEqual(self.settings.read_bytes(), original_settings)

    def test_missing_input_group_is_rejected(self):
        (self.dictionary / "foreign-names" / "synthetic.csv").unlink()
        with self.assertRaisesRegex(ValueError, "入力がありません"):
            dictionary_inputs.verify()

    def test_clean_ci_checkout_and_local_edits(self):
        (self.dictionary / "user" / "synthetic.csv").write_text("changed\n")
        self.assertEqual(dictionary_inputs.verify(), self.expected)
        with self.assertRaisesRegex(ValueError, "未コミット"):
            dictionary_inputs.verify(clean=True)

    def test_branch_ref(self):
        self.assertEqual(dictionary_inputs.ref(), "refs/heads/main")

    def test_invalid_branch_is_rejected(self):
        for branch in [None, "", "-main", "../main", "main..other", "main/"]:
            with self.subTest(branch=branch):
                self.settings.write_text(
                    json.dumps(
                        {
                            "repository": "example/hasami-dictionaries",
                            "branch": branch,
                        }
                    )
                )
                with self.assertRaisesRegex(ValueError, "取得ブランチが不正"):
                    dictionary_inputs.ref()

    def test_legacy_commit_only_settings_are_rejected(self):
        self.settings.write_text(
            json.dumps(
                {
                    "repository": "example/hasami-dictionaries",
                    "commit": self.expected,
                }
            )
        )
        with self.assertRaisesRegex(ValueError, "取得ブランチが不正"):
            dictionary_inputs.settings()

    def test_repository_name_is_validated(self):
        self.settings.write_text(
            json.dumps(
                {
                    "repository": "https://example.invalid/other",
                    "branch": "main",
                }
            )
        )
        with self.assertRaisesRegex(ValueError, "owner/name"):
            dictionary_inputs.settings()


class DictionaryWordSelectionTests(unittest.TestCase):
    def setUp(self):
        self.workspace = tempfile.TemporaryDirectory()
        self.addCleanup(self.workspace.cleanup)
        self.root = Path(self.workspace.name)
        self.inputs = self.root / "input"
        self.inputs.mkdir()
        self.output = self.root / "selected.csv"

    def row(self, surface="合成語", reading="ゴウセイゴ"):
        return (
            surface,
            "+1",
            "2",
            "-123",
            "名詞",
            "一般",
            "*",
            "*",
            "*",
            "*",
            surface,
            reading,
            reading,
        )

    def write_rows(self, name, rows):
        with (self.inputs / name).open("w", encoding="utf-8", newline="") as stream:
            csv.writer(stream).writerows(rows)

    def read_output(self):
        with self.output.open(encoding="utf-8", newline="") as stream:
            return [tuple(row) for row in csv.reader(stream)]

    def test_exact_selection_preserves_variants_quotes_and_source_order(self):
        first = self.row("合成語", "ゴウセイゴ")
        quoted = list(self.row("合成語", "ベツヨミ"))
        quoted[10] = '別名,"引用"'
        quoted = tuple(quoted)
        second = self.row("別の語")
        self.write_rows("z.csv", [first, quoted, second])
        self.write_rows("a.csv", [first, self.row("合成語の続き")])
        original = {path.name: path.read_bytes() for path in self.inputs.glob("*.csv")}
        select_dictionary_words.write_selection(
            self.inputs, ["合成語", "別の語", "合成語"], self.output
        )
        self.assertEqual(self.read_output(), [first, quoted, second])
        before = self.output.read_bytes()
        select_dictionary_words.write_selection(
            self.inputs, ["別の語", "合成語"], self.output
        )
        self.assertEqual(self.output.read_bytes(), before)
        self.assertEqual(
            original,
            {path.name: path.read_bytes() for path in self.inputs.glob("*.csv")},
        )

    def test_missing_word_fails_without_replacing_output(self):
        self.write_rows("words.csv", [self.row()])
        self.output.write_bytes(b"previous output")
        with self.assertRaisesRegex(ValueError, "不足"):
            select_dictionary_words.write_selection(
                self.inputs, ["合成語", "未登録"], self.output
            )
        self.assertEqual(self.output.read_bytes(), b"previous output")

    def test_invalid_selected_schema_ids_and_cost_fail_before_output(self):
        for row in [self.row()[:-1], self.row() + ("余分",)]:
            with self.subTest(columns=len(row)):
                self.write_rows("words.csv", [row])
                with self.assertRaisesRegex(ValueError, "13列"):
                    select_dictionary_words.write_selection(
                        self.inputs, ["合成語"], self.output
                    )
                self.assertFalse(self.output.exists())
        for field, value in [
            (1, "-1"),
            (1, "65536"),
            (2, "文字"),
            (2, "１"),
            (3, "32768"),
            (3, "-32769"),
            (3, " 123"),
        ]:
            with self.subTest(field=field, value=value):
                row = list(self.row())
                row[field] = value
                self.write_rows("words.csv", [row])
                with self.assertRaisesRegex(ValueError, "不正"):
                    select_dictionary_words.write_selection(
                        self.inputs, ["合成語"], self.output
                    )
                self.assertFalse(self.output.exists())

    def test_empty_inputs_requests_and_output_in_input_directory_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "入力CSV"):
            select_dictionary_words.write_selection(
                self.inputs, ["合成語"], self.output
            )
        self.write_rows("words.csv", [self.row()])
        for words in [[], [""], ["合成語", ""]]:
            with self.subTest(words=words), self.assertRaisesRegex(ValueError, "空"):
                select_dictionary_words.write_selection(self.inputs, words, self.output)
        original = (self.inputs / "words.csv").read_bytes()
        with self.assertRaisesRegex(ValueError, "外"):
            select_dictionary_words.write_selection(
                self.inputs, ["合成語"], self.inputs / "words.csv"
            )
        self.assertEqual((self.inputs / "words.csv").read_bytes(), original)

    def test_csv_syntax_error_is_rejected_without_exposing_values(self):
        (self.inputs / "words.csv").write_text(
            '合成語,"sensitive-value', encoding="utf-8"
        )
        result = subprocess.run(
            [
                sys.executable,
                str(Path(select_dictionary_words.__file__)),
                "--input-dir",
                str(self.inputs),
                "--word",
                "合成語",
                "--output",
                str(self.output),
            ],
            capture_output=True,
            check=False,
            text=True,
            encoding="utf-8",
            env={**os.environ, "PYTHONIOENCODING": "utf-8"},
        )
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertNotIn("sensitive-value", result.stderr)
        self.assertIn("不正", result.stderr)
        self.assertFalse(self.output.exists())


if __name__ == "__main__":
    unittest.main()
