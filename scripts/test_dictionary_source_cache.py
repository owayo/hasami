"""上流Gitキャッシュの保護を、所有済み一時repoと実Bashで検証する。"""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

HELPER = Path(__file__).resolve().with_name("dictionary_source_cache.sh")


class DictionarySourceCacheTests(unittest.TestCase):
    def setUp(self):
        self.workspace = tempfile.TemporaryDirectory(prefix="hasami-source-cache-")
        self.addCleanup(self.workspace.cleanup)
        self.root = Path(self.workspace.name).resolve()
        self.bash = shutil.which("bash")
        self.assertIsNotNone(self.bash, "Git for Windows等のBashが必要")
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        self.env.update(
            GIT_CONFIG_NOSYSTEM="1",
            GIT_CONFIG_GLOBAL=os.devnull,
            GIT_AUTHOR_NAME="Source cache test",
            GIT_AUTHOR_EMAIL="test@example.invalid",
            GIT_COMMITTER_NAME="Source cache test",
            GIT_COMMITTER_EMAIL="test@example.invalid",
            GIT_TERMINAL_PROMPT="0",
            GIT_ALLOW_PROTOCOL="file",
        )
        self.upstream = self.root / "upstream"
        self.upstream.mkdir()
        self.git(self.upstream, "init", "--quiet", "-b", "main")
        (self.upstream / "seed").mkdir()
        (self.upstream / "seed/one.csv.xz").write_bytes(b"synthetic first seed\n")
        (self.upstream / ".gitignore").write_text("seed/ignored*\n", encoding="utf-8")
        (self.upstream / "README").write_text("synthetic upstream\n", encoding="utf-8")
        (self.upstream / "other").mkdir()
        (self.upstream / "other/unused").write_bytes(b"outside sparse input\n")
        self.git(self.upstream, "add", ".")
        self.git(self.upstream, "commit", "--quiet", "-m", "First synthetic source")
        self.first = self.git(self.upstream, "rev-parse", "HEAD")
        (self.upstream / "seed/one.csv.xz").write_bytes(b"synthetic second seed\n")
        (self.upstream / "seed/two.csv.xz").write_bytes(b"synthetic extra seed\n")
        self.git(self.upstream, "add", ".")
        self.git(self.upstream, "commit", "--quiet", "-m", "Second synthetic source")
        self.second = self.git(self.upstream, "rev-parse", "HEAD")
        self.cache = self.root / "cache"
        self.git(
            self.root,
            "clone",
            "--quiet",
            "--no-hardlinks",
            str(self.upstream),
            str(self.cache),
        )
        self.git(self.cache, "checkout", "--quiet", "--detach", self.first)

    def git(self, directory, *args):
        self.assertTrue(directory.resolve().is_relative_to(self.root))
        return subprocess.check_output(
            ["git", "-c", "advice.detachedHead=false", "-C", str(directory), *args],
            env=self.env,
            text=True,
            encoding="utf-8",
        ).strip()

    def fetch(
        self, commit=None, directory=None, required="seed", repo=None, no_transport=True
    ):
        command = 'source "$1"; log() { :; }; '
        if no_transport:
            command += """git() {
  for arg in "$@"; do
    case "$arg" in clone|fetch) printf 'unexpected transport\\n' >&2; return 91;; esac
  done
  command git "$@"
}; """
        command += 'fetch_git "$2" "$3" "$4" "$5"'
        return subprocess.run(
            [
                self.bash,
                "-euo",
                "pipefail",
                "-c",
                command,
                "--",
                HELPER.as_posix(),
                str(repo or self.upstream),
                commit or self.first,
                str(directory or self.cache),
                required,
            ],
            check=False,
            env=self.env,
            capture_output=True,
            text=True,
            encoding="utf-8",
        )

    def snapshot(self, path):
        self.assertTrue(path.resolve().is_relative_to(self.root))
        if path.is_symlink():
            return {"symlink": os.readlink(path)}
        if path.is_file():
            return {"file": path.read_bytes()}
        result = {}
        for directory, dirs, files in os.walk(path, followlinks=False):
            for name in dirs + files:
                item = Path(directory) / name
                key = item.relative_to(path).as_posix()
                if item.is_symlink():
                    result[key] = ("symlink", os.readlink(item))
                elif item.is_file():
                    result[key] = item.read_bytes()
                else:
                    result[key] = None
        return result

    def test_deleted_tracked_seed_is_rejected_without_losing_cache(self):
        (self.cache / "seed/one.csv.xz").unlink()
        before = self.snapshot(self.cache)
        result = self.fetch()
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertIn("dirty", result.stderr)
        self.assertNotIn("unexpected transport", result.stderr)
        self.assertEqual(self.snapshot(self.cache), before)

    def assert_rejected_and_preserved(
        self, directory=None, commit=None, reason=None, **kwargs
    ):
        directory = directory or self.cache
        before = self.snapshot(directory)
        result = self.fetch(directory=directory, commit=commit, **kwargs)
        self.assertNotEqual(result.returncode, 0, result.stderr)
        if reason:
            self.assertIn(reason, result.stderr)
        self.assertNotIn("unexpected transport", result.stderr)
        self.assertEqual(self.snapshot(directory), before)

    def test_clean_same_sha_reuses_without_transport_or_metadata_changes(self):
        before = self.snapshot(self.cache)
        result = self.fetch()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.snapshot(self.cache), before)

    def test_staged_modified_untracked_and_ignored_inputs_are_preserved_even_for_other_sha(
        self,
    ):
        for state in ["modified", "staged", "staged-deletion", "untracked", "ignored"]:
            with self.subTest(state=state):
                directory = self.root / state
                self.git(
                    self.root,
                    "clone",
                    "--quiet",
                    "--no-hardlinks",
                    str(self.cache),
                    str(directory),
                )
                target = directory / "seed/one.csv.xz"
                if state in {"modified", "staged"}:
                    target.write_bytes(b"local edited seed\n")
                    if state == "staged":
                        self.git(directory, "add", "seed/one.csv.xz")
                elif state == "staged-deletion":
                    self.git(directory, "rm", "--quiet", "seed/one.csv.xz")
                else:
                    name = (
                        "ignored-local.csv.xz" if state == "ignored" else "extra.csv.xz"
                    )
                    (directory / "seed" / name).write_bytes(b"local added seed\n")
                self.assert_rejected_and_preserved(
                    directory, self.second, reason="dirty"
                )

    def test_untracked_and_ignored_files_outside_seed_are_also_preserved(self):
        (self.cache / "local-notes").write_bytes(b"local notes\n")
        self.assert_rejected_and_preserved(reason="dirty")
        (self.cache / "local-notes").unlink()
        (self.cache / ".git/info/exclude").write_text(
            "local-ignored\n", encoding="utf-8"
        )
        (self.cache / "local-ignored").write_bytes(b"ignored local notes\n")
        self.assert_rejected_and_preserved(reason="dirty")

    def test_nonrepo_file_directory_and_parent_repo_child_are_never_removed(self):
        ordinary = self.root / "ordinary"
        ordinary.mkdir()
        (ordinary / "keep").write_bytes(b"ordinary directory contents\n")
        file = self.root / "ordinary-file"
        file.write_bytes(b"ordinary file contents\n")
        child = self.upstream / "not-a-checkout"
        child.mkdir()
        (child / "keep").write_bytes(b"parent repo is not this cache\n")
        invalid = self.root / "invalid-git"
        invalid.mkdir()
        (invalid / ".git").mkdir()
        for path in [ordinary, file, child, invalid]:
            with self.subTest(path=path.name):
                self.assert_rejected_and_preserved(path)

    def test_cache_and_git_directory_symlinks_are_rejected_without_following_them(self):
        link = self.root / "cache-link"
        try:
            link.symlink_to(self.cache, target_is_directory=True)
        except OSError as error:
            self.skipTest(f"directory symlinks unavailable: {error}")
        original = self.snapshot(self.cache)
        self.assert_rejected_and_preserved(link, reason="independent checkout")
        self.assertEqual(self.snapshot(self.cache), original)
        saved_git = self.root / "saved-git"
        self.assertTrue(saved_git.resolve().is_relative_to(self.root))
        (self.cache / ".git").rename(saved_git)
        (self.cache / ".git").symlink_to(saved_git, target_is_directory=True)
        self.assert_rejected_and_preserved(reason="independent checkout")
        self.assertTrue((self.cache / ".git").is_symlink())

    def test_linked_worktree_is_rejected_and_parent_checkout_is_unchanged(self):
        linked = self.root / "linked"
        self.git(
            self.cache,
            "worktree",
            "add",
            "--quiet",
            "--detach",
            str(linked),
            self.first,
        )
        before = self.snapshot(self.cache)
        self.assert_rejected_and_preserved(linked, reason="independent checkout")
        self.assertEqual(self.snapshot(self.cache), before)

    def test_separate_common_git_directory_is_rejected(self):
        (self.cache / ".git/commondir").write_text(
            self.upstream.joinpath(".git").as_posix() + "\n", encoding="utf-8"
        )
        self.assert_rejected_and_preserved(reason="unexpected")

    def test_assume_unchanged_modified_or_missing_input_is_rejected(self):
        self.git(self.cache, "update-index", "--assume-unchanged", "seed/one.csv.xz")
        target = self.cache / "seed/one.csv.xz"
        target.write_bytes(b"hidden local change\n")
        self.assert_rejected_and_preserved(
            self.cache, self.second, reason="assume-unchanged"
        )
        target.unlink()
        self.assert_rejected_and_preserved(reason="assume-unchanged")

    def test_skip_worktree_missing_or_present_required_input_is_rejected(self):
        self.git(self.cache, "update-index", "--skip-worktree", "seed/one.csv.xz")
        self.assert_rejected_and_preserved(reason="hidden index flags")
        (self.cache / "seed/one.csv.xz").unlink()
        self.assert_rejected_and_preserved(reason="hidden index flags")

    def test_sparse_checkout_requires_materialized_input_but_allows_excluded_other_paths(
        self,
    ):
        self.git(self.cache, "sparse-checkout", "set", "seed")
        self.assertFalse((self.cache / "other/unused").exists())
        before = self.snapshot(self.cache)
        result = self.fetch()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.snapshot(self.cache), before)
        self.git(self.cache, "sparse-checkout", "set", "other")
        self.assertFalse((self.cache / "seed/one.csv.xz").exists())
        self.assert_rejected_and_preserved(reason="required input")

    def test_missing_or_invalid_required_subtree_never_updates_cache(self):
        for required in ["missing", "README", "../upstream", "seed/../other"]:
            with self.subTest(required=required):
                self.assert_rejected_and_preserved(
                    commit=self.second, required=required
                )

    def test_clean_update_keeps_detached_head_local_branches_and_origin(self):
        self.git(self.cache, "branch", "local-work", self.first)
        (self.cache / "seed/one.csv.xz").write_bytes(b"local committed seed\n")
        self.git(self.cache, "add", "seed/one.csv.xz")
        self.git(self.cache, "commit", "--quiet", "-m", "Local detached source")
        old_head = self.git(self.cache, "rev-parse", "HEAD")
        old_refs = self.git(self.cache, "show-ref")
        config = (self.cache / ".git/config").read_bytes()
        result = self.fetch(commit=self.second)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.git(self.cache, "rev-parse", "HEAD"), self.second)
        self.assertEqual(
            self.git(self.cache, "rev-parse", f"refs/hasami/source-cache/{old_head}"),
            old_head,
        )
        refs = self.git(self.cache, "show-ref").splitlines()
        for ref in old_refs.splitlines():
            self.assertIn(ref, refs)
        self.assertEqual((self.cache / ".git/config").read_bytes(), config)
        self.assertEqual(
            self.git(self.cache, "show", f"{old_head}:seed/one.csv.xz"),
            "local committed seed",
        )
        self.assertEqual(
            (self.cache / "seed/two.csv.xz").read_bytes(), b"synthetic extra seed\n"
        )

    def test_clean_update_fetches_only_from_explicit_local_repository(self):
        old_refs = self.git(self.cache, "show-ref")
        config = (self.cache / ".git/config").read_bytes()
        (self.upstream / "seed/third.csv.xz").write_bytes(b"new synthetic source\n")
        self.git(self.upstream, "add", ".")
        self.git(self.upstream, "commit", "--quiet", "-m", "Third synthetic source")
        third = self.git(self.upstream, "rev-parse", "HEAD")
        result = self.fetch(commit=third, no_transport=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.git(self.cache, "rev-parse", "HEAD"), third)
        self.assertEqual(
            (self.cache / "seed/third.csv.xz").read_bytes(), b"new synthetic source\n"
        )
        self.assertEqual((self.cache / ".git/config").read_bytes(), config)
        for ref in old_refs.splitlines():
            self.assertIn(ref, self.git(self.cache, "show-ref").splitlines())

    def test_new_cache_materializes_fixed_commit_and_required_sparse_input(self):
        new = self.root / "new-cache"
        result = self.fetch(directory=new, no_transport=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.git(new, "rev-parse", "HEAD"), self.first)
        self.assertTrue((new / "seed/one.csv.xz").is_file())
        self.assertFalse((new / "seed/two.csv.xz").exists())
        self.assertFalse((new / "other/unused").exists())
        before = self.snapshot(new)
        result = self.fetch(directory=new)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.snapshot(new), before)

    def test_conflicting_backup_ref_is_preserved_and_blocks_update(self):
        self.git(
            self.cache,
            "update-ref",
            f"refs/hasami/source-cache/{self.first}",
            self.second,
        )
        self.assert_rejected_and_preserved(commit=self.second, reason="backup ref")

    def test_repository_environment_override_is_rejected_without_using_other_index(
        self,
    ):
        self.env["GIT_INDEX_FILE"] = str(self.root / "external-index")
        self.assert_rejected_and_preserved(reason="Git repository override")
        self.assertFalse((self.root / "external-index").exists())

    def test_failed_local_fetch_keeps_previous_head_files_and_refs(self):
        before_head = self.git(self.cache, "rev-parse", "HEAD")
        before_refs = self.git(self.cache, "show-ref").splitlines()
        before_seed = (self.cache / "seed/one.csv.xz").read_bytes()
        config = (self.cache / ".git/config").read_bytes()
        result = self.fetch(commit="f" * 40, no_transport=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.git(self.cache, "rev-parse", "HEAD"), before_head)
        self.assertEqual((self.cache / "seed/one.csv.xz").read_bytes(), before_seed)
        self.assertEqual((self.cache / ".git/config").read_bytes(), config)
        for ref in before_refs:
            self.assertIn(ref, self.git(self.cache, "show-ref").splitlines())


if __name__ == "__main__":
    unittest.main()
