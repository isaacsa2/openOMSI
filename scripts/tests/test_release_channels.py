"""Exercise the real promotion workflow offline; gh records calls instead of publishing."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
SUFFIXES = "windows-x64.zip windows-arm64.zip macos-arm64.zip macos-x64.zip linux-x64.zip linux-arm64.zip android-arm64.apk server-linux-x64.zip server-linux-arm64.zip server-windows-x64.zip server-windows-arm64.zip".split()


class Promotion(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / "scripts").mkdir()
        shutil.copy(ROOT / "scripts/release-notes.sh", self.root / "scripts/release-notes.sh")
        (self.root / "release").mkdir()
        (self.root / "release/health.json").write_text('{"bad": []}')
        (self.root / "CHANGELOG.md").write_text("# Changelog\n\n## 0.2.48\nPrevious stable.\n")
        self.git("init", "-q")
        self.git("add", ".")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "Stable fixture")
        self.git("tag", "v0.2.48")
        (self.root / "CHANGELOG.md").write_text("# Changelog\n\n## 0.2.51\nNewest change.\n\n## 0.2.50\nEarlier beta change.\n\n## 0.2.48\nPrevious stable.\n")
        self.git("add", ".")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "Beta fixture")
        self.git("tag", "v0.2.51")
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")
        self.bin = self.root / "bin"
        self.bin.mkdir()
        gh = self.bin / "gh"
        gh.write_text('''#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
with open(os.environ["GH_TEST_CALLS"], "a") as log:
    log.write(json.dumps(args) + "\\n")
if args[:2] == ["release", "view"]:
    field = args[args.index("--json") + 1]
    if field == "isDraft,isPrerelease": print(os.environ.get("GH_TEST_STATE", "false true"))
    elif field == "assets": print(os.environ["GH_TEST_FILES"])
    elif field == "tagName": print("v0.2.48")
    else: sys.exit("unexpected view")
elif args[:2] != ["release", "edit"]:
    sys.exit("unexpected gh mutation")
''')
        gh.chmod(0o755)
        workflow = yaml.safe_load((ROOT / ".github/workflows/promote.yml").read_text())
        self.script = next(step["run"] for step in workflow["jobs"]["promote"]["steps"] if "run" in step)
        self.calls = self.root / "calls.jsonl"
        self.env = {
            **os.environ,
            "PATH": str(self.bin) + os.pathsep + os.environ["PATH"],
            "VERSION": "0.2.51",
            "GITHUB_STEP_SUMMARY": str(self.root / "summary.md"),
            "GH_TEST_CALLS": str(self.calls),
            "GH_TEST_FILES": "\n".join([*(f"openOMSI-0.2.51-{suffix}" for suffix in SUFFIXES), "openOMSI-android.apk", "SHA256SUMS"]),
        }

    def tearDown(self):
        self.temp.cleanup()

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.root, text=True, capture_output=True, check=True)

    def promote(self):
        return subprocess.run(["bash", "-e", "-o", "pipefail", "-c", self.script], cwd=self.root, env=self.env, capture_output=True, text=True)

    def mutations(self):
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()] if self.calls.exists() else []
        return [call for call in calls if call[:2] == ["release", "edit"]]

    def test_valid_promotion_edits_only_metadata_and_accumulates_notes(self):
        before = self.git("rev-parse", "v0.2.51").stdout
        result = self.promote()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.mutations()
        self.assertEqual(len(calls), 1)
        self.assertIn("--prerelease=false", calls[0])
        self.assertIn("--latest", calls[0])
        self.assertIn("openOMSI 0.2.51 — Stable", calls[0])
        self.assertEqual(before, self.git("rev-parse", "v0.2.51").stdout)
        notes = (self.root / "notes.md").read_text()
        self.assertIn("Stable channel — early release", notes)
        self.assertIn("Newest change.", notes)
        self.assertIn("Earlier beta change.", notes)
        self.assertNotIn("Previous stable.", notes)

    def test_malformed_versions_are_refused_before_any_github_call(self):
        for value in ["", "..", "0..2", "0.2", "0.2.51; echo pwned", "0.2.51$(id)", "0.2.51\n0.2.52"]:
            self.env["VERSION"] = value
            self.assertNotEqual(self.promote().returncode, 0, value)
            self.assertFalse(self.calls.exists(), value)

    def test_missing_platform_is_not_promoted(self):
        self.env["GH_TEST_FILES"] = self.env["GH_TEST_FILES"].replace("openOMSI-0.2.51-linux-arm64.zip", "")
        self.assertNotEqual(self.promote().returncode, 0)
        self.assertEqual(self.mutations(), [])

    def test_missing_checksums_are_not_promoted(self):
        self.env["GH_TEST_FILES"] = self.env["GH_TEST_FILES"].replace("SHA256SUMS", "")
        self.assertNotEqual(self.promote().returncode, 0)
        self.assertEqual(self.mutations(), [])

    def test_filename_matching_is_literal(self):
        self.env["GH_TEST_FILES"] = self.env["GH_TEST_FILES"].replace("openOMSI-0.2.51-linux-arm64.zip", "openOMSI-0x2x51-linux-arm64.zip")
        self.assertNotEqual(self.promote().returncode, 0)
        self.assertEqual(self.mutations(), [])

    def test_invalid_health_list_is_not_promoted(self):
        (self.root / "release/health.json").write_text('invalid JSON')
        self.assertNotEqual(self.promote().returncode, 0)
        self.assertEqual(self.mutations(), [])

    def test_known_broken_beta_is_not_promoted(self):
        (self.root / "release/health.json").write_text('{"bad": [" v0.2.51 "]}')
        self.assertNotEqual(self.promote().returncode, 0)
        self.assertEqual(self.mutations(), [])

    def test_draft_or_already_stable_is_not_promoted(self):
        for state in ["true true", "false false"]:
            self.env["GH_TEST_STATE"] = state
            self.assertNotEqual(self.promote().returncode, 0)
            self.assertEqual(self.mutations(), [])

    def test_beta_notes_name_the_channel_and_keep_the_content_requirement(self):
        result = subprocess.run(["sh", "scripts/release-notes.sh", "0.2.51"], cwd=self.root, text=True, capture_output=True, check=True)
        self.assertIn("openOMSI 0.2.51 — Beta", result.stdout)
        self.assertIn("Beta channel — testing release", result.stdout)
        self.assertIn("original OMSI 2 installation", result.stdout)
        self.assertNotIn("Earlier beta change.", result.stdout)


if __name__ == "__main__":
    unittest.main()
