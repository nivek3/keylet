import os
from pathlib import Path
import signal
import sys
import tempfile
import threading
import time
import unittest

import hashlib

from keylet.keylet import (
    _get_key_pair_for_test,
    _get_signer_public_key_for_test,
    _sign_with_signer_for_test,
)
from public_helper import SIGNER_PAYLOAD, SIGNERS, VECTORS


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

    def test_sigint_interrupts_main_thread_and_allows_retry(self):
        ready = self.helper.with_suffix(".ready")
        stop = threading.Event()

        def interrupt():
            while not ready.exists():
                if stop.wait(0.01):
                    return
            os.kill(os.getpid(), signal.SIGINT)

        sender = threading.Thread(target=interrupt)
        sender.start()
        try:
            started = time.monotonic()
            with self.assertRaises(KeyboardInterrupt):
                self.get_pair("interrupt")
            self.assertLess(time.monotonic() - started, 5)
        finally:
            stop.set()
            sender.join()

        pair = self.get_pair("trading")
        self.assertEqual(pair.public_key, "0x" + VECTORS["secp256k1"][2])
        self.assertFalse(self.helper.with_suffix(".expired").exists())


class PublicSignerTest(unittest.TestCase):
    DIGEST = hashlib.sha256(SIGNER_PAYLOAD).digest()

    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        script = Path(__file__).with_name("public_helper.py").read_text()
        self.helper = Path(folder.name) / "signer"
        self.helper.write_text(f"#!{sys.executable}\n{script}")
        self.helper.chmod(0o700)

    def sign(self, name, payload, **options):
        return _sign_with_signer_for_test(str(self.helper), name=name, payload=payload, **options)

    def starts(self, name):
        log = self.helper.with_suffix(".starts")
        lines = log.read_text().splitlines() if log.exists() else []
        return [line for line in lines if line.split(" ")[1] == name]

    def test_both_curves_return_verified_signatures(self):
        ed = self.sign("sol", SIGNER_PAYLOAD)
        self.assertEqual((ed.curve, ed.recovery_id), ("ed25519", None))
        self.assertEqual(ed.signature, SIGNERS["ed25519"]["signature"])
        self.assertEqual(ed.address, SIGNERS["ed25519"]["address"])
        secp = self.sign("evm", self.DIGEST)
        self.assertEqual((secp.curve, secp.address), ("secp256k1", None))
        self.assertEqual(secp.public_key, SIGNERS["secp256k1"]["publicKey"])
        self.assertEqual(secp.recovery_id, 1)
        self.assertNotIn(secp.signature, repr(secp))

    def test_calls_share_a_session_per_signer_and_auth_ttl(self):
        for _ in range(3):
            self.sign("evm-session", self.DIGEST, auth_ttl=60)
        self.sign("evm-session", self.DIGEST)
        self.assertEqual(
            self.starts("evm-session"),
            ["session evm-session --ttl 60", "session evm-session"],
        )

    def test_unverifiable_signatures_and_app_errors_end_the_session(self):
        for name, payload, code in (
            ("sol-tamper", b"other bytes", "INVALID_HELPER_RESPONSE"),
            ("evm-tamper", bytes(32), "INVALID_HELPER_RESPONSE"),
            ("cancel", SIGNER_PAYLOAD, "CANCELLED"),
            ("missing", SIGNER_PAYLOAD, "NOT_FOUND"),
        ):
            with self.subTest(name=name):
                for _ in range(2):
                    with self.assertRaisesRegex(RuntimeError, f"^{code}$"):
                        self.sign(name, payload)
                self.assertEqual(len(self.starts(name)), 2)

    def test_public_key(self):
        key = _get_signer_public_key_for_test(str(self.helper), name="sol-key")
        self.assertEqual((key.curve, key.address), ("ed25519", SIGNERS["ed25519"]["address"]))
        key = _get_signer_public_key_for_test(str(self.helper), name="evm-key")
        self.assertEqual((key.public_key, key.address), (SIGNERS["secp256k1"]["publicKey"], None))
        self.assertEqual(self.starts("evm-key"), ["public-key evm-key"])

    def test_invalid_requests_fail_before_launching(self):
        for name, payload, options in (
            ("invalid", b"", {}),
            ("invalid", bytes(1233), {}),
            ("bad\n", b"x", {}),
            ("invalid", b"x", {"auth_ttl": -1}),
            ("invalid", b"x", {"auth_ttl": True}),
        ):
            with self.subTest(name=name, size=len(payload), options=options):
                with self.assertRaisesRegex(ValueError, "^Invalid signer request$"):
                    self.sign(name, payload, **options)
        with self.assertRaises(TypeError):
            self.sign("invalid", "not bytes")
        self.assertEqual(self.starts("invalid"), [])


if __name__ == "__main__":
    unittest.main()
