import { getKeyPair } from 'keylet';

const [name, path] = process.argv.slice(2);
if (!name || !path) throw new Error('Pass a saved wallet name and its full hardened derivation path');
const { publicKey } = await getKeyPair({ name, curve: 'ed25519', path });
console.log({ publicKey }); // 32-byte Ed25519 public key in hex, not a base58 Solana address.
