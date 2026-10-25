"""Prepare only public bundle/signing metadata. Does not read any wallet."""
import datetime
import pathlib
import plistlib
import re
import shutil
import sys

profile_path, bundle_id, app_path, version = sys.argv[1:]
if not re.fullmatch(r"[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+", bundle_id):
    raise SystemExit("Invalid bundle id")
if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version):
    raise SystemExit("App version must be a numeric major.minor.patch version")
with open(profile_path, "rb") as source:
    profile = plistlib.load(source)
expiration = profile.get("ExpirationDate")
if not expiration or expiration <= datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None):
    raise SystemExit("Provisioning profile is expired or invalid")
entitlements = profile.get("Entitlements", {})
allowed_id = entitlements.get("com.apple.application-identifier", "")
prefixes = profile.get("ApplicationIdentifierPrefix", [])
teams = profile.get("TeamIdentifier", [])
if len(prefixes) != 1 or len(teams) != 1:
    raise SystemExit("Expected one app identifier prefix and team")
app_id = prefixes[0] + "." + bundle_id


def allows(pattern, value):
    return pattern == value or (pattern.endswith(".*") and value.startswith(pattern[:-1]))


if not allows(allowed_id, app_id):
    raise SystemExit("Profile does not authorize this bundle id")
if not any(allows(group, app_id) for group in entitlements.get("keychain-access-groups", [])):
    raise SystemExit("Profile does not authorize the app's private keychain group")
app = pathlib.Path(app_path)
info = {
    "CFBundleIdentifier": bundle_id,
    "CFBundleName": "Keylet",
    "CFBundleDisplayName": "Keylet",
    "CFBundleExecutable": "keylet",
    "CFBundleIconFile": "AppIcon.icns",
    "CFBundlePackageType": "APPL",
    "CFBundleVersion": version,
    "CFBundleShortVersionString": version,
    "LSMinimumSystemVersion": "14.0",
    "LSUIElement": False,
    "WalletAccessGroup": app_id,
}
signed_entitlements = {
    "com.apple.application-identifier": app_id,
    "com.apple.developer.team-identifier": teams[0],
    "keychain-access-groups": [app_id],
}
resources = app / "Contents/Resources"
resources.mkdir(parents=True, exist_ok=True)
shutil.copyfile(pathlib.Path(__file__).resolve().parents[1] / "assets/AppIcon.icns",
                resources / "AppIcon.icns")
for path, value in [(app / "Contents/Info.plist", info),
                    (app.parent / "entitlements.plist", signed_entitlements)]:
    with path.open("wb") as destination:
        plistlib.dump(value, destination)
