import os
from pathlib import Path
import sys
import tempfile
import unittest

from keylet.keylet import _get_key_pair_for_test
from public_helper import VECTORS


class PublicHelperTest(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        script = Path(__file__).with_name("public_helper.py").read_text()
        self.helper = Path(folder.name) / "helper"
        self.helper.write_text(f"#!{sys.executable}\n{script}")
        self.helper.chmod(0o700)

    def get_pair(self, name, curve="secp256k1", **selection):
        return _get_key_pair_for_test(
            str(self.helper), name=name, curve=curve, auth_ttl=0, **selection
        )

    def test_public_key_pairs_and_paths_match_node_fixtures(self):
        for curve, (path, private, public) in VECTORS.items():
            with self.subTest(curve=curve):
                pair = self.get_pair("trading", curve, path=path)
                self.assertEqual(pair.public_key, "0x" + public)
                self.assertEqual(pair.private_key, "0x" + private)
                self.assertNotIn(private, repr(pair))
        pair = self.get_pair("trading", index=0)
        _, private, public = VECTORS["secp256k1"]
        self.assertEqual(pair.public_key, "0x" + public)
        self.assertEqual(pair.private_key, "0x" + private)

    def test_errors_end_session_and_a_later_call_can_retry(self):
        for name, code in (("cancel", "CANCELLED"), ("mismatch", "INVALID_HELPER_RESPONSE")):
            with self.subTest(name=name):
                with self.assertRaisesRegex(RuntimeError, f"^{code}$"):
                    self.get_pair(name, index=0)
                self.assertEqual(self.get_pair("trading").private_key, "0x" + VECTORS["secp256k1"][1])

    def test_forked_child_cannot_use_parent_session(self):
        self.get_pair("trading")
        pid = os.fork()
        if pid == 0:
            try:
                self.get_pair("trading")
            except RuntimeError as error:
                os._exit(0 if str(error) == "FORKED_PROCESS" else 1)
            os._exit(1)
        _, status = os.waitpid(pid, 0)
        self.assertEqual(os.waitstatus_to_exitcode(status), 0)


if __name__ == "__main__":
    unittest.main()
