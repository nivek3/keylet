import json
import os
import sys

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


def main():
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
        if name == "mismatch":
            public = "00"
        os.write(3, bytes.fromhex(private))
        print(json.dumps({**request, "publicKey": "0x" + public}), flush=True)


if __name__ == "__main__":
    main()
