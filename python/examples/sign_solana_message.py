"""Sign bytes with a local Ed25519 signer, as you would a Solana transaction message.

Usage: python sign_solana_message.py <signer> [message]

With solders or solana-py, pass the serialized transaction message bytes as the payload and
attach the returned signature for `signed.address`. The private key stays in the Keylet app.
"""

import sys

from keylet import sign_with_signer

ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


def base58(data: bytes) -> str:
    number, text = int.from_bytes(data, "big"), ""
    while number:
        number, digit = divmod(number, 58)
        text = ALPHABET[digit] + text
    return "1" * (len(data) - len(data.lstrip(b"\x00"))) + text


def main() -> None:
    name = sys.argv[1]
    message = (sys.argv[2] if len(sys.argv) > 2 else "Signed by a Keylet local signer").encode()
    # auth_ttl lets later calls in this process skip the prompt, up to the app's setting.
    signed = sign_with_signer(name=name, payload=message, auth_ttl=60)
    print({"address": signed.address, "signature": base58(bytes.fromhex(signed.signature[2:]))})


if __name__ == "__main__":
    main()
