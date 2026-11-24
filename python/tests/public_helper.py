import json
import os
from pathlib import Path
import sys
import time

# Public fixtures shared with the Node tests. Never fund these keys.
VECTORS = {
    "secp256k1": (
        "m/44'/60'/0'/0/0",
        "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
        "038318535b54105d4a7aae60c08fc45f9687181b4fdfc625bd1a753fa7397fed75",
    ),
    "ed25519": (
        "m/44'/501'/0'/0'",
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
    ),
}


def fail(code):
    print(code, file=sys.stderr)
    sys.exit(1)


# Signatures precomputed with independent libraries (Node crypto and ethers)
# over SIGNER_PAYLOAD (Ed25519) and its SHA-256 digest (secp256k1).
SIGNER_PAYLOAD = b"keylet python fixture"
SIGNERS = {
    "ed25519": {
        "publicKey": "0x" + VECTORS["ed25519"][2],
        "address": "FVen3X669xLzsi6N2V91DoiyzHzg1uAgqiT8jZ9nS96Z",
        "signature": "0x86583ef5a432087188ee40fe17450679b5b91fe5a661de4988fd0a769f0c797f"
        "9533e3a15da2c63572c45ec2e48a6222243b6c6afcd8a8812f5fe61ea66a1901",
    },
    "secp256k1": {
        "publicKey": "0x" + VECTORS["secp256k1"][2],
        "signature": "0x1c16d5a086e6b5cd734fcc2217e879c747e1f4ad22c710099b3500999fefc3a4"
        "49c0b57c7bf182314d997841f0b12af87c25fc3ae3cc415b1fbb0688893b4219",
        "recoveryId": 1,
    },
}


def signer(operation, name, options):
    with Path(__file__).with_suffix(".starts").open("a") as log:
        print(" ".join([operation, name, *options]), file=log)
    curve = "secp256k1" if name.startswith("evm") else "ed25519"
    response = {"name": name, "curve": curve, **SIGNERS[curve]}
    if name.startswith("missing"):
        fail("NOT_FOUND")
    if operation == "public-key":
        response.pop("signature")
        response.pop("recoveryId", None)
        print(json.dumps(response), flush=True)
        return
    for _ in sys.stdin:
        if name.startswith("cancel"):
            fail("CANCELLED")
        # Always answers with the precomputed signature; other payloads fail verification.
        print(json.dumps(response), flush=True)


def main():
    if sys.argv[1:2] == ["signer"] and len(sys.argv) >= 4:
        signer(sys.argv[2], sys.argv[3], sys.argv[4:])
        return
    if sys.argv[1:] != ["--bridge", "0"]:
        fail("INVALID_INPUT")
    for line in sys.stdin:
        request = json.loads(line)
        name = request["name"]
        path, private, public = VECTORS[request["curve"]]
        if request["path"] != path:
            fail("INVALID_INPUT")
        if name == "cancel":
            fail("CANCELLED")
        if name == "interrupt":
            Path(__file__).with_suffix(".ready").touch()
            time.sleep(5)
            Path(__file__).with_suffix(".expired").touch()
            fail("CANCELLED")
        if name == "mismatch":
            public = "00"
        os.write(3, bytes.fromhex(private))
        print(json.dumps({**request, "publicKey": "0x" + public}), flush=True)


if __name__ == "__main__":
    main()
