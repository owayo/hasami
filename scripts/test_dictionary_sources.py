"""最新版の選択、破損拒否、差分検出と設定の保存をネットワークなしで確かめる。"""

import copy
import hashlib
import io
import json
import subprocess
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch
from urllib.parse import parse_qs, urlparse

import dictionary_sources as sources


def listing(directories=(), files=(), truncated=False, token=None):
    root = sources.ET.Element("ListBucketResult", xmlns=sources.S3_NS["s3"])
    sources.ET.SubElement(root, "IsTruncated").text = str(truncated).lower()
    for directory in directories:
        item = sources.ET.SubElement(root, "CommonPrefixes")
        sources.ET.SubElement(item, "Prefix").text = directory
    for file in files:
        item = sources.ET.SubElement(root, "Contents")
        sources.ET.SubElement(item, "Key").text = file
    if token is not None:
        sources.ET.SubElement(root, "NextContinuationToken").text = token
    return sources.ET.tostring(root)


def raw_zip(name, text="語彙,読み\n"):
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as archive:
        archive.writestr(name, text)
    return buffer.getvalue()


class DictionarySourcesTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.path = self.root / "dictionary-sources.json"
        self.original = sources.load_sources(sources.SOURCES_PATH)
        self.path.write_text(json.dumps(self.original), encoding="utf-8")

    def latest_mocks(self, version=None, hashes=None):
        stack = self.enterContext(patch.object(sources, "latest_commit"))
        stack.side_effect = [
            self.original["ipadic"]["commit"],
            self.original["neologd"]["commit"],
        ]
        self.enterContext(
            patch.object(
                sources,
                "latest_raw_version",
                return_value=version or self.original["sudachi"]["version"],
            )
        )
        return self.enterContext(
            patch.object(
                sources,
                "download_raw",
                side_effect=hashes or list(self.original["sudachi"]["sha256"].values()),
            )
        )

    def test_shell_settings_work_in_bash(self):
        command = sources.shell_settings(sources.load_sources(self.path))
        command += '\nprintf "%s\\n" "$IPADIC_COMMIT" "$NEOLOGD_COMMIT" "$SUDACHI_VERSION" "${SUDACHI_FILES[@]}"'
        result = subprocess.run(
            ["bash", "-eu", "-c", command], check=True, capture_output=True, text=True
        )
        self.assertEqual(
            result.stdout.splitlines(),
            [
                self.original["ipadic"]["commit"],
                self.original["neologd"]["commit"],
                self.original["sudachi"]["version"],
                *[
                    f"{name} {self.original['sudachi']['sha256'][name]}"
                    for name in sources.SUDACHI_FILES
                ],
            ],
        )

    def test_invalid_settings_never_become_shell_code(self):
        for name, value in (
            ("commit", "$(exit 99)"),
            ("version", "20260723/../../bad"),
            ("hash", "0;exit 99"),
        ):
            with self.subTest(name=name):
                invalid = copy.deepcopy(self.original)
                if name == "commit":
                    invalid["ipadic"]["commit"] = value
                elif name == "version":
                    invalid["sudachi"]["version"] = value
                else:
                    invalid["sudachi"]["sha256"]["small_lex.zip"] = value
                self.path.write_text(json.dumps(invalid), encoding="utf-8")
                with self.assertRaises(ValueError):
                    sources.load_sources(self.path)

    def test_month_date_and_revision_sort_in_calendar_order(self):
        self.assertLess(
            sources.version_key("20260723.1"), sources.version_key("202610")
        )
        self.assertLess(
            sources.version_key("20260723.2"), sources.version_key("20260723.10")
        )
        with self.assertRaises(ValueError):
            sources.version_key("20261301")

    def test_s3_listing_reads_every_page(self):
        pages = [
            listing(directories=["first/"], truncated=True, token="next+/="),
            listing(directories=["second/"], files=["second/core_lex.zip"]),
        ]
        with patch.object(sources, "curl", side_effect=pages) as request:
            directories, files = sources.list_raw(sources.SUDACHI_PREFIX)
        self.assertEqual(directories, ["first/", "second/"])
        self.assertIn("second/core_lex.zip", files)
        query = parse_qs(urlparse(request.call_args_list[1].args[0]).query)
        self.assertEqual(query["continuation-token"], ["next+/="])

    def test_truncated_listing_without_continuation_fails(self):
        with (
            patch.object(sources, "curl", return_value=listing(truncated=True)),
            self.assertRaises(ValueError),
        ):
            sources.list_raw(sources.SUDACHI_PREFIX)

    def test_repeated_continuation_fails(self):
        with (
            patch.object(
                sources, "curl", return_value=listing(truncated=True, token="same")
            ),
            self.assertRaises(ValueError),
        ):
            sources.list_raw(sources.SUDACHI_PREFIX)

    def test_latest_raw_skips_incomplete_and_invalid_directories(self):
        prefix = sources.SUDACHI_PREFIX

        def page(url, destination=None):
            requested = parse_qs(urlparse(url).query)["prefix"][0]
            if requested == prefix:
                return listing(
                    directories=[
                        prefix + "20260723/",
                        prefix + "20261002/",
                        prefix + "202610/",
                        prefix + "latest/",
                        "other/20991231/",
                    ]
                )
            if requested == prefix + "20261002/":
                return listing(files=[requested + "small_lex.zip"])
            self.assertEqual(requested, prefix + "202610/")
            return listing(files=[requested + name for name in sources.SUDACHI_FILES])

        with patch.object(sources, "curl", side_effect=page):
            self.assertEqual(sources.latest_raw_version(), "202610")

    def test_no_complete_raw_fails(self):
        with (
            patch.object(sources, "list_raw", return_value=([], set())),
            self.assertRaises(ValueError),
        ):
            sources.latest_raw_version()

    def test_download_hashes_the_archive_and_installs_it(self):
        contents = raw_zip("small_lex.csv")

        def download(url, destination):
            destination.write_bytes(contents)

        with patch.object(sources, "curl", side_effect=download):
            digest = sources.download_raw(
                "20260723", "small_lex.zip", self.root / "raw"
            )
        self.assertEqual(digest, hashlib.sha256(contents).hexdigest())
        self.assertEqual((self.root / "raw/small_lex.zip").read_bytes(), contents)

    def test_bad_zip_preserves_cached_file_and_removes_partial(self):
        directory = self.root / "raw"
        directory.mkdir()
        cached = directory / "small_lex.zip"
        cached.write_bytes(b"previous archive")

        def download(url, destination):
            destination.write_bytes(b"not a zip")

        with (
            patch.object(sources, "curl", side_effect=download),
            self.assertRaises(zipfile.BadZipFile),
        ):
            sources.download_raw("20260723", "small_lex.zip", directory)
        self.assertEqual(cached.read_bytes(), b"previous archive")
        self.assertEqual(list(directory.iterdir()), [cached])

    def test_unexpected_zip_paths_are_rejected(self):
        for filename in ("../small_lex.csv", "/small_lex.csv", "core_lex.csv"):
            with self.subTest(filename=filename):
                contents = raw_zip(filename)

                def download(url, destination, contents=contents):
                    destination.write_bytes(contents)

                with (
                    patch.object(sources, "curl", side_effect=download),
                    self.assertRaises(ValueError),
                ):
                    sources.download_raw("20260723", "small_lex.zip", self.root / "raw")

    def test_unchanged_sources_do_not_rewrite_the_file(self):
        original_bytes = self.path.read_bytes()
        self.latest_mocks()
        self.assertFalse(sources.update_sources(self.path, self.root))
        self.assertEqual(self.path.read_bytes(), original_bytes)

    def test_new_raw_version_and_hashes_are_recorded_together(self):
        self.latest_mocks(version="20991231", hashes=["a" * 64, "b" * 64])
        self.assertTrue(sources.update_sources(self.path, self.root))
        saved = sources.load_sources(self.path)
        self.assertEqual(saved["sudachi"]["version"], "20991231")
        self.assertEqual(
            saved["sudachi"]["sha256"],
            {"small_lex.zip": "a" * 64, "core_lex.zip": "b" * 64},
        )

    def test_changed_hash_under_the_same_version_is_detected(self):
        self.latest_mocks(hashes=["a" * 64, "b" * 64])
        self.assertTrue(sources.update_sources(self.path, self.root))
        self.assertEqual(
            sources.load_sources(self.path)["sudachi"]["version"],
            self.original["sudachi"]["version"],
        )

    def test_new_upstream_commit_is_detected(self):
        self.latest_mocks()
        sources.latest_commit.side_effect = [
            "a" * 40,
            self.original["neologd"]["commit"],
        ]
        self.assertTrue(sources.update_sources(self.path, self.root))
        self.assertEqual(sources.load_sources(self.path)["ipadic"]["commit"], "a" * 40)

    def test_partial_failure_preserves_source_versions(self):
        original_bytes = self.path.read_bytes()
        self.latest_mocks(
            version="20991231", hashes=["a" * 64, OSError("download failed")]
        )
        with self.assertRaises(OSError):
            sources.update_sources(self.path, self.root)
        self.assertEqual(self.path.read_bytes(), original_bytes)

    def test_downgrade_is_rejected(self):
        download = self.latest_mocks(version="20000101")
        with self.assertRaises(ValueError):
            sources.update_sources(self.path, self.root)
        download.assert_not_called()


if __name__ == "__main__":
    unittest.main()
