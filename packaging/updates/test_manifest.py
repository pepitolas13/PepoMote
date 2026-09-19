"""Release contract tests; fixture artifacts never install or contact a device."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
import subprocess
import sys

import manifest


class ManifestTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.notes = {"es": ["Actualiza desde la app.", "Consulta las novedades."],
                      "en": ["Update from the app.", "Read what's new."]}
        self.names = {
            "windows-x86_64": "PepoMote.exe",
            "linux-x86_64": "PepoMote-linux-x86_64",
            "linux-x86_64-appimage": "PepoMote-x86_64.AppImage",
            "macos-aarch64": "PepoMote-macOS.zip",
            "mobile-linux-aarch64": "PepoMote-Mobile-aarch64",
            "mobile-linux-aarch64-appimage": "PepoMote-Mobile-aarch64.AppImage",
            "mobile-linux-aarch64-musl": "PepoMote-Mobile-aarch64-musl",
            "android-universal": "PepoMote.apk",
            "ios": "PepoMote.ipa",
        }
        for name in self.names.values():
            (self.root / name).write_bytes((name + "\n").encode())

    def build(self, tag="v1.11.0", notes=None):
        return manifest.build_manifest(self.root, tag, notes if notes is not None else self.notes,
                                       "2026-09-19T12:00:00Z")

    def test_all_platforms_have_exact_tag_urls_and_real_hashes(self):
        result = self.build()
        self.assertEqual(result["schema"], 1)
        self.assertEqual(result["version"], "1.11.0")
        self.assertEqual(result["notes"], self.notes)
        self.assertEqual(result["release_url"], "https://github.com/pepitolas13/PepoMote/releases/tag/v1.11.0")
        self.assertEqual(set(result["assets"]), set(self.names))
        for target, name in self.names.items():
            data = (self.root / name).read_bytes()
            self.assertEqual(result["assets"][target], {
                "name": name, "size": len(data), "sha256": hashlib.sha256(data).hexdigest(),
                "url": "https://github.com/pepitolas13/PepoMote/releases/download/v1.11.0/" + name,
            })

    def test_missing_or_empty_platform_blocks_entire_release(self):
        for name in self.names.values():
            path = self.root / name
            data = path.read_bytes()
            path.unlink()
            with self.assertRaises(ValueError):
                self.build()
            path.write_bytes(b"")
            with self.assertRaises(ValueError):
                self.build()
            path.write_bytes(data)

    def test_rejects_noncanonical_or_prerelease_tags(self):
        for tag in ["1.2.3", "v1.2", "v1.2.3-beta", "v01.2.3", "v1.2.3/../x", "v1.2.3\n", "v2147483648.0.0"]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                self.build(tag=tag)

    def test_requires_short_notes_in_both_languages(self):
        for notes in [{}, {"es": ["Hola"]}, {"es": [], "en": ["Hi"]},
                      {"es": ["x"] * 9, "en": ["Hi"]}, {"es": ["x" * 281], "en": ["Hi"]},
                      {"es": [42], "en": ["Hi"]}, {"es": ["a\nb"], "en": ["Hi"]},
                      {"es": ["  "], "en": ["Hi"]}]:
            with self.subTest(notes=notes), self.assertRaises(ValueError):
                self.build(notes=notes)

    def test_rejects_invalid_timestamp(self):
        with self.assertRaises(ValueError):
            manifest.build_manifest(self.root, "v1.11.0", self.notes, "yesterday")

    def test_rejects_control_and_line_separator_characters_rejected_by_clients(self):
        for character in ["\x7f", "\x85", "\x9f", "\u2028", "\u2029", "\u200e", "\u200d"]:
            with self.subTest(character=repr(character)), self.assertRaises(ValueError):
                self.build(notes={"es": ["a" + character + "b"], "en": ["Update."]})

    def test_roundtrip_verification_rejects_changed_artifact(self):
        result = self.build()
        manifest.verify_manifest(self.root, result)
        (self.root / "PepoMote.apk").write_bytes(b"corrupted")
        with self.assertRaises(ValueError):
            manifest.verify_manifest(self.root, result)

    def test_roundtrip_verification_rejects_retargeted_url(self):
        result = self.build()
        result["assets"]["windows-x86_64"]["url"] = "https://evil.example/PepoMote.exe"
        with self.assertRaises(ValueError):
            manifest.verify_manifest(self.root, result)

    def test_writer_produces_utf8_and_checksum_for_manifest(self):
        result = self.build()
        manifest.write_release_metadata(self.root, result, list(self.names.values()))
        actual = json.loads((self.root / "update.json").read_text(encoding="utf-8"))
        self.assertEqual(actual, result)
        entries = (self.root / "SHA256SUMS.txt").read_text().splitlines()
        self.assertEqual(len(entries), len(self.names) + 1)
        expected = hashlib.sha256((self.root / "update.json").read_bytes()).hexdigest() + "  update.json"
        self.assertIn(expected, entries)

    def test_release_cli_checks_every_additional_package(self):
        notes = self.root / "notes.json"
        notes.write_text(json.dumps(self.notes), encoding="utf-8")
        command = [sys.executable, str(Path(manifest.__file__).resolve()), "--tag", "v1.11.0",
                   "--notes", str(notes), "--artifacts", str(self.root)]
        incomplete = subprocess.run(command, capture_output=True, text=True)
        self.assertNotEqual(incomplete.returncode, 0)
        self.assertFalse((self.root / "update.json").exists())
        for name in ["PepoMote-linux-x86_64.tar.gz", "PepoMote-macOS.dmg", "altstore.json",
                     "PepoMote-Mobile-aarch64-musl.tar.gz", "pepomote-mobile_1.11.0_arm64.deb",
                     "pepomote-mobile_arm64.deb"]:
            (self.root / name).write_bytes(b"release artifact")
        complete = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(complete.returncode, 0, complete.stderr)
        manifest.verify_manifest(self.root, json.loads((self.root / "update.json").read_text(encoding="utf-8")))

    def test_artifacts_cannot_exceed_download_limit(self):
        path = self.root / "PepoMote.ipa"
        # No allocation of a GiB: temporarily lower the production threshold.
        original = manifest.MAX_PACKAGE
        try:
            manifest.MAX_PACKAGE = path.stat().st_size - 1
            with self.assertRaises(ValueError):
                self.build()
        finally:
            manifest.MAX_PACKAGE = original

    def test_checked_in_release_summaries_follow_the_contract(self):
        releases = Path(__file__).resolve().parents[2] / "docs" / "releases"
        for path in releases.glob("*.json"):
            with self.subTest(path=path.name):
                manifest.validate_notes(json.loads(path.read_text(encoding="utf-8")))


if __name__ == "__main__":
    unittest.main()
