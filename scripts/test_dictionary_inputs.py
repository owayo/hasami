"""非公開辞書のブランチ指定と入力の検証を実際の Git で確かめる。"""

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


if __name__ == "__main__":
    unittest.main()
