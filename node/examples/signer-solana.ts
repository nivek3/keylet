// Sign bytes with a local Ed25519 signer, as you would a Solana transaction message.
// With @solana/web3.js, pass `transaction.serializeMessage()` as the payload, then call
// `transaction.addSignature(new PublicKey(address), Buffer.from(signature.slice(2), 'hex'))`.
import { createPublicKey, verify } from 'node:crypto';
import { signWithSigner } from 'keylet';

const [name, text = 'Signed by a Keylet local signer'] = process.argv.slice(2);
if (!name) throw new Error('Pass an Ed25519 signer name, e.g. one made with `keylet signer create sol`');

const payload = new TextEncoder().encode(text); // 1 to 1232 bytes.
const { address, publicKey, signature } = await signWithSigner({ name, payload });

// Check the signature independently with Node's built-in Ed25519 support.
const spki = Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), Buffer.from(publicKey.slice(2), 'hex')]);
const key = createPublicKey({ key: spki, format: 'der', type: 'spki' });
const verified = verify(null, payload, key, Buffer.from(signature.slice(2), 'hex'));
console.log({ address, signature, verified });
