// Sign a Bitcoin sighash with a local secp256k1 signer and encode it for a P2WPKH witness.
// Compute the BIP-143 sighash with your Bitcoin library, then pass it here as 64 hex digits.
import { secp256k1 } from '@noble/curves/secp256k1.js';
import { signWithSigner } from 'keylet';

const [name, sighash] = process.argv.slice(2);
if (!name || !/^(0x)?[0-9a-fA-F]{64}$/.test(sighash ?? '')) throw new Error('Pass a secp256k1 signer name and a 32-byte sighash in hex');

const { curve, publicKey, signature } = await signWithSigner({ name, payload: `0x${sighash.replace(/^0x/, '')}` });
if (curve !== 'secp256k1') throw new Error('The signer must use secp256k1');

const der = secp256k1.Signature.fromCompact(signature.slice(2)).toDERHex();
const SIGHASH_ALL = '01';

// A P2WPKH input's witness is [DER signature + sighash type, compressed public key].
console.log({ witness: [der + SIGHASH_ALL, publicKey.slice(2)] });
