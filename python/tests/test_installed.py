import unittest

from keylet import get_key_pair, get_signer_public_key, keylet, sign_with_signer


class InstalledWheelTest(unittest.TestCase):
    def test_release_has_no_helper_override(self):
        for name in (
            "_get_key_pair_for_test",
            "_sign_with_signer_for_test",
            "_get_signer_public_key_for_test",
        ):
            self.assertFalse(hasattr(keylet, name), name)

    def test_invalid_signer_requests_fail_before_app_discovery(self):
        for request in (
            {"name": "agent", "payload": b""},
            {"name": "agent", "payload": bytes(1233)},
            {"name": "", "payload": b"x"},
            {"name": "agent", "payload": b"x", "auth_ttl": -1},
            {"name": "agent", "payload": b"x", "auth_ttl": 2**53},
        ):
            with self.subTest(request=request):
                with self.assertRaisesRegex(ValueError, "^Invalid signer request$"):
                    sign_with_signer(**request)
        with self.assertRaisesRegex(ValueError, "^Invalid signer request$"):
            get_signer_public_key(name="bad\n")

    def test_invalid_selections_fail_before_app_discovery(self):
        for selection in (
            {"name": "", "curve": "secp256k1"},
            {"name": "trading", "curve": "secp256k1", "index": -1},
            {"name": "trading", "curve": "secp256k1", "index": True},
            {"name": "trading", "curve": "secp256k1", "index": 2**31},
            {"name": "trading", "curve": "secp256k1", "index": 2**64},
            {"name": "trading", "curve": "secp256k1", "index": 0, "path": "m/0"},
            {"name": "trading", "curve": "secp256k1", "auth_ttl": 2**53},
            {"name": "trading", "curve": "ed25519", "index": 0},
            {"name": "trading", "curve": "ed25519", "path": "m/0"},
        ):
            with self.subTest(selection=selection):
                with self.assertRaisesRegex(ValueError, "^Invalid wallet selection$"):
                    get_key_pair(**selection)


if __name__ == "__main__":
    unittest.main()
