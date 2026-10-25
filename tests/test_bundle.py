"""Public synthetic profile metadata only. Does not sign code or touch Keychain."""
import datetime
import pathlib
import plistlib
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "scripts/prepare-bundle.py"


class BundleTests(unittest.TestCase):
    def prepare(self, authorized_group="PREFIX.com.example.Wallet", expired=False, version="0.1.0"):
        folder = tempfile.TemporaryDirectory(prefix="wallet-public-profile-test-")
        self.addCleanup(folder.cleanup)
        root = pathlib.Path(folder.name)
        app = root / "Keylet.app"
        (app / "Contents").mkdir(parents=True)
        profile = {
            "ApplicationIdentifierPrefix": ["PREFIX"], "TeamIdentifier": ["DIFFERENT_TEAM"],
            "ExpirationDate": datetime.datetime(2000 if expired else 2099, 1, 1),
            "Entitlements": {"com.apple.application-identifier": "PREFIX.com.example.*",
                             "keychain-access-groups": [authorized_group]},
        }
        source = root / "public-profile.plist"
        with source.open("wb") as file:
            plistlib.dump(profile, file)
        result = subprocess.run([sys.executable, str(SCRIPT), str(source), "com.example.Wallet", str(app), version],
                                capture_output=True, check=False)
        return result, root, app

    def test_uses_app_identifier_prefix_and_only_private_group(self):
        result, root, app = self.prepare()
        self.assertEqual(result.returncode, 0, result.stderr)
        with (root / "entitlements.plist").open("rb") as source:
            entitlements = plistlib.load(source)
        self.assertEqual(entitlements, {
            "com.apple.application-identifier": "PREFIX.com.example.Wallet",
            "com.apple.developer.team-identifier": "DIFFERENT_TEAM",
            "keychain-access-groups": ["PREFIX.com.example.Wallet"],
        })
        with (app / "Contents/Info.plist").open("rb") as source:
            info = plistlib.load(source)
        self.assertEqual(info["WalletAccessGroup"], "PREFIX.com.example.Wallet")
        self.assertEqual(info["CFBundleName"], "Keylet")
        self.assertEqual(info["CFBundleDisplayName"], "Keylet")
        self.assertEqual(info["CFBundleExecutable"], "keylet")
        self.assertEqual(info["CFBundleIconFile"], "AppIcon.icns")
        icon = (app / "Contents/Resources/AppIcon.icns").read_bytes()
        self.assertEqual(icon[:4], b"icns")
        self.assertEqual(int.from_bytes(icon[4:8], "big"), len(icon))
        self.assertEqual(icon, (SCRIPT.parent.parent / "assets/AppIcon.icns").read_bytes())
        self.assertFalse(info["LSUIElement"])

    def test_bundle_version_follows_package_version(self):
        result, _, app = self.prepare(version="1.2.3")
        self.assertEqual(result.returncode, 0, result.stderr)
        with (app / "Contents/Info.plist").open("rb") as source:
            info = plistlib.load(source)
        self.assertEqual(info["CFBundleVersion"], "1.2.3")
        self.assertEqual(info["CFBundleShortVersionString"], "1.2.3")

    def test_rejects_invalid_bundle_version(self):
        for version in ["", "v1.2.3", "1.2.3-beta.1", "1.2", "01.2.3"]:
            with self.subTest(version=version):
                result, _, app = self.prepare(version=version)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((app / "Contents/Info.plist").exists())

    def test_rejects_unauthorized_or_expired_profiles(self):
        for group, expired in [("PREFIX.other.*", False), ("PREFIX.com.example.*", True)]:
            with self.subTest(group=group, expired=expired):
                result, _, app = self.prepare(group, expired)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((app / "Contents/Info.plist").exists())


if __name__ == "__main__":
    unittest.main()
