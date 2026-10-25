"""Exercise packaging with fake tool output; never compile or sign real code."""
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "scripts/build-bridge.sh"


class BridgePackagingTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory(prefix="wallet-bridge-packaging-")
        self.addCleanup(folder.cleanup)
        self.root = pathlib.Path(folder.name)
        (self.root / "scripts").mkdir()
        shutil.copyfile(SCRIPT, self.root / "scripts/build-bridge.sh")
        self.output = self.root / "node/dist/keylet.node"
        self.output.parent.mkdir(parents=True)
        self.output.write_text("previous addon")
        tools = self.root / "tools"
        tools.mkdir()
        stub = f"#!{sys.executable}\n" + '''
import os, pathlib, sys
tool = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
if os.environ.get("FAIL_TOOL") == tool:
    sys.exit(1)
if tool == "uname":
    print("Darwin")
elif tool == "cargo":
    target = args[args.index("--target") + 1]
    output = pathlib.Path(os.environ["CARGO_TARGET_DIR"]) / target / "release/libwallet_bridge.dylib"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(target + "\\n")
elif tool == "lipo":
    if args[0] == "-create":
        pathlib.Path(args[-1]).write_text("".join(pathlib.Path(p).read_text() for p in args[1:args.index("-output")]))
    else:
        assert args[:3] == ["-verify_arch", "arm64", "x86_64"]
        data = pathlib.Path(args[-1]).read_text()
        assert "aarch64-apple-darwin" in data and "x86_64-apple-darwin" in data
elif tool == "codesign":
    assert args[:3] == ["--force", "--sign", "-"]
'''
        for name in ["uname", "cargo", "lipo", "codesign"]:
            path = tools / name
            path.write_text(stub)
            path.chmod(0o700)
        self.env = {**os.environ, "PATH": str(tools) + os.pathsep + os.environ["PATH"]}
        self.env.pop("FAIL_TOOL", None)

    def pack(self, fail_tool=""):
        return subprocess.run(
            ["/bin/bash", str(self.root / "scripts/build-bridge.sh"), "--universal"],
            env={**self.env, "FAIL_TOOL": fail_tool}, capture_output=True, check=False,
        )

    def test_release_contains_both_architectures(self):
        result = self.pack()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.output.read_text(), "aarch64-apple-darwin\nx86_64-apple-darwin\n")
        self.assertEqual(list(self.output.parent.glob(".bridge.*")), [])

    def test_failed_release_preserves_previous_addon(self):
        for tool in ["cargo", "lipo", "codesign"]:
            with self.subTest(tool=tool):
                result = self.pack(tool)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.output.read_text(), "previous addon")
                self.assertEqual(list(self.output.parent.glob(".bridge.*")), [])
