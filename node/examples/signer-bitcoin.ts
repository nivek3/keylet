// Sign a Bitcoin sighash with a local secp256k1 signer and encode it for a P2WPKH witness.
// Compute the BIP-143 sighash with your Bitcoin library, then pass it here as 64 hex digits.
import { signWithSigner } from 'keylet';

const [name, sighash] = process.argv.slice(2);
if (!name || !/^(0x)?[0-9a-fA-F]{64}$/.test(sighash ?? '')) throw new Error('Pass a secp256k1 signer name and a 32-byte sighash in hex');

const { publicKey, signature } = await signWithSigner({ name, payload: `0x${sighash.replace(/^0x/, '')}` });

// DER-encode r and s (Keylet already returns low-S, as Bitcoin requires).
const integer = (bytes: Buffer) => {
  let value = bytes.subarray(bytes.findIndex((byte) => byte !== 0));
  if (value[0] & 0x80) value = Buffer.concat([Buffer.from([0]), value]);
  return Buffer.concat([Buffer.from([0x02, value.length]), value]);
};
const raw = Buffer.from(signature.slice(2), 'hex');
const body = Buffer.concat([integer(raw.subarray(0, 32)), integer(raw.subarray(32))]);
const SIGHASH_ALL = 0x01;
const der = Buffer.concat([Buffer.from([0x30, body.length]), body, Buffer.from([SIGHASH_ALL])]);

// A P2WPKH input's witness is [signature, compressed public key].
console.log({ witness: [der.toString('hex'), publicKey.slice(2)] });
