"""Sign bytes with a local Ed25519 signer, as you would a Solana transaction message.

Usage: python sign_solana_message.py <signer> [message]

With solders or solana-py, pass the serialized transaction message bytes as the payload and
attach the returned signature for `signed.address`. The private key stays in the Keylet app.
"""

import sys

from base58 import b58encode
from keylet import sign_with_signer

def main() -> None:
    name = sys.argv[1]
    message = (sys.argv[2] if len(sys.argv) > 2 else "Signed by a Keylet local signer").encode()
    # auth_ttl lets later calls in this process skip the prompt, up to the app's setting.
    signed = sign_with_signer(name=name, payload=message, auth_ttl=60)
    if signed.curve != "ed25519":
        raise ValueError("The signer must use Ed25519")
    signature = b58encode(bytes.fromhex(signed.signature[2:])).decode("ascii")
    print({"address": signed.address, "signature": signature})


if __name__ == "__main__":
    main()
