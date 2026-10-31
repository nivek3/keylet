// In your project, install ethers and import Wallet from 'ethers'.
import { Wallet } from 'ethers-v6';
import { getKeyPair } from 'keylet';

const name = process.argv[2];
if (!name) throw new Error('Pass the name of a wallet already saved in the app');
const index = process.argv[3] === undefined ? 0 : Number(process.argv[3]);
const { privateKey } = await getKeyPair({ name, curve: 'secp256k1', index });
const signer = new Wallet(privateKey);
console.log({ address: signer.address });
