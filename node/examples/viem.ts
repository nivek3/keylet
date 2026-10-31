import { privateKeyToAccount } from 'viem/accounts';
import { getKeyPair } from 'keylet';

const name = process.argv[2];
if (!name) throw new Error('Pass the name of a wallet already saved in the app');
const index = process.argv[3] === undefined ? 0 : Number(process.argv[3]);
const { privateKey } = await getKeyPair({ name, curve: 'secp256k1', index });
const signer = privateKeyToAccount(privateKey);
console.log({ address: signer.address });
