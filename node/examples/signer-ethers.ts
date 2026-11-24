// Sign an Ethereum transaction and a personal message with a local secp256k1 signer.
// The private key stays in the Keylet app; only digests go in and signatures come out.
// In your project, install ethers and import from 'ethers'.
import { Signature, Transaction, computeAddress, hashMessage, verifyMessage } from 'ethers-v6';
import { getSignerPublicKey, signWithSigner, type SignerSignature } from 'keylet';

const name = process.argv[2];
if (!name) throw new Error('Pass a secp256k1 signer name, e.g. one made with `keylet signer create evm --curve secp256k1`');

const toEthers = (signed: SignerSignature) =>
  Signature.from({ r: signed.signature.slice(0, 66), s: `0x${signed.signature.slice(66)}`, v: 27 + signed.recoveryId! });

const { publicKey } = await getSignerPublicKey(name);
const address = computeAddress(publicKey);
console.log({ address });

// Build the transaction yourself; Keylet signs the hash blindly. Verify it before approving.
const tx = Transaction.from({
  type: 2,
  chainId: 11155111, // Sepolia
  nonce: 0,
  to: address,
  value: 0n,
  gasLimit: 21000n,
  maxFeePerGas: 30_000_000_000n,
  maxPriorityFeePerGas: 1_000_000_000n,
});
// authTTL lets later calls in this process skip the prompt, up to the app's "Reuse signer auth" setting.
tx.signature = toEthers(await signWithSigner({ name, payload: tx.unsignedHash, authTTL: 60 }));
if (tx.from !== address) throw new Error('Signature does not match the signer');
console.log({ serializedTransaction: tx.serialized }); // Broadcast with any provider.

const message = 'Signed by a Keylet local signer';
const signature = toEthers(await signWithSigner({ name, payload: hashMessage(message), authTTL: 60 }));
console.log({ message, signature: signature.serialized, verified: verifyMessage(message, signature) === address });
