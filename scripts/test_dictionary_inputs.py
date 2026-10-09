"""公開 checkout と非公開辞書の版が一致するかを実際の Git で確かめる。"""

import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import dictionary_inputs


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
                    "commit": self.expected,
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

    def test_different_commit_is_rejected(self):
        (self.dictionary / "user" / "synthetic.csv").write_text("changed\n")
        self.commit_dictionary()
        with self.assertRaisesRegex(ValueError, "参照コミットと異な"):
            dictionary_inputs.verify()

    def test_missing_input_group_is_rejected(self):
        (self.dictionary / "foreign-names" / "synthetic.csv").unlink()
        with self.assertRaisesRegex(ValueError, "入力がありません"):
            dictionary_inputs.verify()

    def test_clean_ci_checkout_and_local_edits(self):
        (self.dictionary / "user" / "synthetic.csv").write_text("changed\n")
        self.assertEqual(dictionary_inputs.verify(), self.expected)
        with self.assertRaisesRegex(ValueError, "未コミット"):
            dictionary_inputs.verify(clean=True)

    def test_unpinned_inputs_are_rejected(self):
        self.settings.write_text(
            json.dumps(
                {
                    "repository": "example/hasami-dictionaries",
                    "commit": "main",
                }
            )
        )
        with self.assertRaisesRegex(ValueError, "参照コミットが不正"):
            dictionary_inputs.revision()

    def test_repository_name_is_validated(self):
        self.settings.write_text(
            json.dumps(
                {
                    "repository": "https://example.invalid/other",
                    "commit": self.expected,
                }
            )
        )
        with self.assertRaisesRegex(ValueError, "owner/name"):
            dictionary_inputs.settings()


if __name__ == "__main__":
    unittest.main()
