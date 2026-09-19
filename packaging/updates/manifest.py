"""Build and verify update.json using only completed release artifacts.

No network calls, dependencies, signing keys or device installation. The SHA-256
checks protect downloads; HTTPS and the official release repository establish
the metadata's origin. This is not an independent cryptographic signature.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import unicodedata

REPO = "pepitolas13/PepoMote"
BASE = f"https://github.com/{REPO}"
MAX_PACKAGE = 1024 * 1024 * 1024
TARGETS = {
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


def version_from_tag(tag):
    if not isinstance(tag, str) or not re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", tag):
        raise ValueError("Expected a canonical stable tag: vX.Y.Z")
    if any(int(n) > 2**31 - 1 for n in tag[1:].split(".")):
        raise ValueError("Version component exceeds the common client limit")
    return tag[1:]


def validate_notes(notes):
    if not isinstance(notes, dict) or set(notes) != {"es", "en"}:
        raise ValueError("Notes must contain es and en lists")
    for language, lines in notes.items():
        if not isinstance(lines, list) or not 1 <= len(lines) <= 8:
            raise ValueError(f"{language}: provide 1 to 8 concise notes")
        for line in lines:
            if (not isinstance(line, str) or not 1 <= len(line.strip()) <= 280
                    or len(line) > 280
                    or any(unicodedata.category(c) in {"Cc", "Cf", "Cs", "Zl", "Zp"} for c in line)):
                raise ValueError(f"{language}: notes must be plain single lines of 1–280 characters")
    return notes


def file_metadata(directory, name):
    root = Path(directory).resolve()
    path = root / name
    if Path(name).name != name or path.is_symlink() or not path.is_file():
        raise ValueError(f"Missing or unsafe artifact: {name}")
    size = path.stat().st_size
    if not 1 <= size <= MAX_PACKAGE:
        raise ValueError(f"Empty or oversized artifact: {name}")
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"name": name, "size": size, "sha256": digest}


def build_manifest(directory, tag, notes, published_at):
    version = version_from_tag(tag)
    validate_notes(notes)
    if not isinstance(published_at, str) or not published_at.endswith("Z"):
        raise ValueError("published_at must be an ISO UTC timestamp")
    datetime.fromisoformat(published_at.replace("Z", "+00:00"))
    assets = {}
    for target, name in TARGETS.items():
        assets[target] = {**file_metadata(directory, name), "url": f"{BASE}/releases/download/{tag}/{name}"}
    return {
        "schema": 1, "version": version, "published_at": published_at,
        "release_url": f"{BASE}/releases/tag/{tag}", "notes": notes, "assets": assets,
    }


def verify_manifest(directory, document):
    try:
        expected = build_manifest(directory, "v" + document["version"], document["notes"], document["published_at"])
    except (TypeError, KeyError) as error:
        raise ValueError("Incomplete manifest") from error
    if expected != document:
        raise ValueError("Manifest does not match the completed release artifacts")


def write_release_metadata(directory, document, names):
    verify_manifest(directory, document)
    # Validate every additional package BEFORE exposing the new manifest.
    names = sorted(set(names) | {a["name"] for a in document["assets"].values()})
    records = [file_metadata(directory, name) for name in names if name != "update.json"]
    data = (json.dumps(document, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    if len(data) > 256 * 1024:
        raise ValueError("Manifest exceeds client size limit")
    root = Path(directory)
    pending = root / "update.json.tmp"
    pending.write_bytes(data)
    pending.replace(root / "update.json")
    records.append(file_metadata(root, "update.json"))
    sums = "".join(f"{r['sha256']}  {r['name']}\n" for r in sorted(records, key=lambda r: r["name"]))
    pending = root / "SHA256SUMS.txt.tmp"
    pending.write_text(sums, encoding="utf-8", newline="\n")
    pending.replace(root / "SHA256SUMS.txt")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--notes", type=Path, required=True)
    parser.add_argument("--artifacts", type=Path, default=Path("."))
    parser.add_argument("--validate-notes", action="store_true")
    args = parser.parse_args()
    version = version_from_tag(args.tag)
    notes = validate_notes(json.loads(args.notes.read_text(encoding="utf-8")))
    if args.validate_notes:
        print(f"Release notes validated for {args.tag}")
        return
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")
    document = build_manifest(args.artifacts, args.tag, notes, stamp)
    extra = ["PepoMote-linux-x86_64.tar.gz", "PepoMote-macOS.dmg", "altstore.json",
             "PepoMote-Mobile-aarch64-musl.tar.gz", f"pepomote-mobile_{version}_arm64.deb",
             "pepomote-mobile_arm64.deb"]
    write_release_metadata(args.artifacts, document, extra)
    print(f"Verified {len(TARGETS)} update targets; wrote update.json and SHA256SUMS.txt")


if __name__ == "__main__":
    main()
