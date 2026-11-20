import unittest

from keylet import get_key_pair, keylet


class InstalledWheelTest(unittest.TestCase):
    def test_release_has_no_helper_override(self):
        self.assertFalse(hasattr(keylet, "_get_key_pair_for_test"))

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
