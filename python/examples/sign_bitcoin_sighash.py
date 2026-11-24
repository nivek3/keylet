"""Sign a Bitcoin sighash with a local secp256k1 signer and encode it for a P2WPKH witness.

Usage: python sign_bitcoin_sighash.py <signer> <32-byte sighash hex>

Compute the BIP-143 sighash with your Bitcoin library. The private key stays in the Keylet app.
"""

import sys

from keylet import sign_with_signer

SIGHASH_ALL = 0x01


def der_integer(value: bytes) -> bytes:
    value = value.lstrip(b"\x00") or b"\x00"
    if value[0] & 0x80:
        value = b"\x00" + value
    return bytes([0x02, len(value)]) + value


def main() -> None:
    name, sighash = sys.argv[1], bytes.fromhex(sys.argv[2].removeprefix("0x"))
    signed = sign_with_signer(name=name, payload=sighash)  # Exactly 32 bytes for secp256k1.
    raw = bytes.fromhex(signed.signature[2:])
    body = der_integer(raw[:32]) + der_integer(raw[32:])  # Keylet returns low-S already.
    der = bytes([0x30, len(body)]) + body + bytes([SIGHASH_ALL])
    print({"witness": [der.hex(), signed.public_key[2:]]})


if __name__ == "__main__":
    main()
