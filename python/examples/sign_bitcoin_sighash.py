"""Sign a Bitcoin sighash with a local secp256k1 signer and encode it for a P2WPKH witness.

Usage: python sign_bitcoin_sighash.py <signer> <32-byte sighash hex>

Compute the BIP-143 sighash with your Bitcoin library. The private key stays in the Keylet app.
"""

import sys

from cryptography.hazmat.primitives.asymmetric.utils import encode_dss_signature
from keylet import sign_with_signer

SIGHASH_ALL = 0x01


def main() -> None:
    name, sighash = sys.argv[1], bytes.fromhex(sys.argv[2].removeprefix("0x"))
    signed = sign_with_signer(name=name, payload=sighash)  # Exactly 32 bytes for secp256k1.
    if signed.curve != "secp256k1":
        raise ValueError("The signer must use secp256k1")
    raw = bytes.fromhex(signed.signature[2:])
    r = int.from_bytes(raw[:32], "big")
    s = int.from_bytes(raw[32:], "big")
    signature = encode_dss_signature(r, s) + bytes([SIGHASH_ALL])
    print({"witness": [signature.hex(), signed.public_key[2:]]})


if __name__ == "__main__":
    main()
