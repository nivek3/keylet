"""Tests for scripts/check-item-spacing.py on synthetic Rust snippets."""
import pathlib
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "scripts/check-item-spacing.py"

SOURCE = """impl A {
    fn a() {}
    fn b() {
        if x {
        }
        let y = 1;
        #[cfg(test)]
        call();
    }
    #[must_use]
    /// Docs.
    pub(crate) const fn c() {}
}
struct S;
"""

FIXED = """impl A {
    fn a() {}

    fn b() {
        if x {
        }
        let y = 1;
        #[cfg(test)]
        call();
    }

    #[must_use]
    /// Docs.
    pub(crate) const fn c() {}
}

struct S;
"""


class ItemSpacingTests(unittest.TestCase):
    def run_script(self, *args):
        return subprocess.run(
            [sys.executable, "-I", str(SCRIPT), *args], capture_output=True, text=True
        )

    def write(self, text):
        folder = tempfile.TemporaryDirectory(prefix="item-spacing-test-")
        self.addCleanup(folder.cleanup)
        path = pathlib.Path(folder.name) / "lib.rs"
        path.write_text(text)
        return path

    def test_reports_items_without_a_blank_line_but_not_statements(self):
        path = self.write(SOURCE)
        result = self.run_script(str(path))
        self.assertEqual(result.returncode, 1)
        reported = [line.split(":")[1] for line in result.stdout.splitlines() if ": add" in line]
        self.assertEqual(reported, ["3", "10", "14"])

    def test_fix_inserts_blank_lines_and_is_idempotent(self):
        path = self.write(SOURCE)
        self.assertEqual(self.run_script("--fix", str(path)).returncode, 0)
        self.assertEqual(path.read_text(), FIXED)
        self.assertEqual(self.run_script(str(path)).returncode, 0)
        self.run_script("--fix", str(path))
        self.assertEqual(path.read_text(), FIXED)


if __name__ == "__main__":
    unittest.main()
