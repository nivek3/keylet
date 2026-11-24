import assert from 'node:assert/strict';
import { createPublicKey, verify } from 'node:crypto';
import { chmodSync, existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { Signature, Transaction, computeAddress, encodeBase58, hashMessage, keccak256, recoverAddress, toUtf8Bytes, verifyMessage } from 'ethers-v6';
import type { SignerPublicKey, SignerSignature } from '../src/index.js';

type NativeRequest = { name: string; payload: string; authTTL?: number };
type TestBridge = {
  signWithSignerForTest(request: NativeRequest, executable: string): Promise<SignerSignature>;
  getSignerPublicKeyForTest(request: { name: string }, executable: string): Promise<SignerPublicKey>;
};
const bridge = createRequire(import.meta.url)('./keylet-test.node') as TestBridge;
const sign = (request: NativeRequest) => bridge.signWithSignerForTest(request, helper);
const macOnly = { skip: process.platform !== 'darwin' };

// Public test keys only. Never fund these accounts.
const hardhatAddress = '0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266';
const packageJson = fileURLToPath(new URL('../package.json', import.meta.url));
const folder = mkdtempSync(join(tmpdir(), 'keylet-public-signer-'));
const helper = join(folder, 'signer.mjs');
// A stand-in for `keylet signer …` that signs with real public test keys.
writeFileSync(
  helper,
  `#!${process.execPath}
import { appendFileSync } from 'node:fs';
import { createPrivateKey, createPublicKey, sign } from 'node:crypto';
import { createRequire } from 'node:module';
import { createInterface } from 'node:readline';
const { SigningKey, encodeBase58 } = createRequire(${JSON.stringify(packageJson)})('ethers-v6');
const [command, operation, name, ...rest] = process.argv.slice(2);
appendFileSync(process.argv[1] + '.starts', [operation, name, ...rest].join(' ') + '\\n');
const fail = (code) => { process.stderr.write(code + '\\n'); process.exit(1); };
if (command !== 'signer' || name.startsWith('missing')) fail('NOT_FOUND');
const ed = !name.startsWith('evm');
const edKey = createPrivateKey({ key: Buffer.from('302e020100300506032b6570042204209d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60', 'hex'), format: 'der', type: 'pkcs8' });
const edPublic = createPublicKey(edKey).export({ format: 'der', type: 'spki' }).subarray(-32);
const secp = new SigningKey('0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80');
const publicInfo = ed
  ? { name, curve: 'ed25519', publicKey: '0x' + edPublic.toString('hex'), address: encodeBase58(edPublic) }
  : { name, curve: 'secp256k1', publicKey: secp.compressedPublicKey };
if (operation === 'public-key') { console.log(JSON.stringify(publicInfo)); process.exit(0); }
for await (const line of createInterface({ input: process.stdin })) {
  appendFileSync(process.argv[1] + '.frames', name + '\\n');
  if (name.startsWith('cancel')) fail('CANCELLED');
  const payload = Buffer.from(JSON.parse(line).payload.slice(2), 'hex');
  if (name.includes('tamper')) payload[0] ^= 1;
  if (ed) {
    console.log(JSON.stringify({ ...publicInfo, signature: '0x' + sign(null, payload, edKey).toString('hex') }));
  } else {
    if (payload.length !== 32) fail('INVALID_INPUT');
    const signature = secp.sign(payload);
    console.log(JSON.stringify({ ...publicInfo, signature: signature.r + signature.s.slice(2), recoveryId: signature.yParity }));
  }
}
`
);
chmodSync(helper, 0o700);
const starts = (name: string) =>
  existsSync(helper + '.starts')
    ? readFileSync(helper + '.starts', 'utf8')
        .split('\n')
        .filter((line) => line.split(' ')[1] === name)
    : [];
const hex = (bytes: Uint8Array) => `0x${Buffer.from(bytes).toString('hex')}`;

test('Ed25519 signer signs raw bytes that verify against the reported Solana address', macOnly, async () => {
  const message = Buffer.from('serialized Solana transaction message (public test)');
  const result = await sign({ name: 'sol-sign', payload: hex(message) });
  assert.equal(result.curve, 'ed25519');
  assert.equal(result.recoveryId ?? undefined, undefined);
  assert.equal(result.publicKey, '0xd75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a');
  assert.equal(result.address, encodeBase58(result.publicKey));
  const key = createPublicKey({ key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), Buffer.from(result.publicKey.slice(2), 'hex')]), format: 'der', type: 'spki' });
  assert.equal(verify(null, message, key, Buffer.from(result.signature.slice(2), 'hex')), true);
});

test('secp256k1 signer output assembles an Ethereum transaction and a personal_sign signature', macOnly, async () => {
  const tx = Transaction.from({ type: 2, chainId: 11155111, nonce: 0, to: hardhatAddress, value: 1n, gasLimit: 21000n, maxFeePerGas: 2n, maxPriorityFeePerGas: 1n });
  const signed = await sign({ name: 'evm-tx', payload: tx.unsignedHash });
  assert.equal(signed.curve, 'secp256k1');
  assert.equal(signed.address ?? undefined, undefined);
  assert.equal(computeAddress(signed.publicKey), hardhatAddress);
  tx.signature = Signature.from({ r: signed.signature.slice(0, 66), s: `0x${signed.signature.slice(66)}`, v: 27 + signed.recoveryId! });
  assert.equal(tx.from, hardhatAddress);
  assert.equal(Transaction.from(tx.serialized).from, hardhatAddress);

  const message = 'public test only';
  const personal = await sign({ name: 'evm-tx', payload: hashMessage(message) });
  const signature = Signature.from({ r: personal.signature.slice(0, 66), s: `0x${personal.signature.slice(66)}`, v: 27 + personal.recoveryId! });
  assert.equal(verifyMessage(message, signature), hardhatAddress);
  assert.equal(recoverAddress(keccak256(toUtf8Bytes('other')), signature) === hardhatAddress, false);
});

test('calls share one session per signer and authTTL', macOnly, async () => {
  const digest = keccak256(toUtf8Bytes('session'));
  for (let i = 0; i < 3; i++) await sign({ name: 'evm-session', payload: digest, authTTL: 60 });
  assert.deepEqual(starts('evm-session'), ['session evm-session --ttl 60']);
  await sign({ name: 'evm-session', payload: digest });
  await sign({ name: 'evm-session', payload: digest, authTTL: 0 });
  assert.deepEqual(starts('evm-session'), ['session evm-session --ttl 60', 'session evm-session', 'session evm-session --ttl 0']);
  const concurrent = await Promise.all([1, 2, 3].map(() => sign({ name: 'evm-session', payload: digest, authTTL: 60 })));
  for (const result of concurrent) assert.deepEqual(result, concurrent[0]);
  assert.equal(starts('evm-session').length, 3);
});

test('app errors and unverifiable signatures end the session; the next call starts a new one', macOnly, async () => {
  const digest = keccak256(toUtf8Bytes('errors'));
  for (const [name, code] of [
    ['cancel-sign', 'CANCELLED'],
    ['missing-sign', 'NOT_FOUND'],
    ['tamper-sign', 'INVALID_HELPER_RESPONSE'],
    ['evm-tamper-sign', 'INVALID_HELPER_RESPONSE'],
  ]) {
    await assert.rejects(sign({ name, payload: digest }), { message: code });
    await assert.rejects(sign({ name, payload: digest }), { message: code });
    assert.equal(starts(name).length, 2, name);
  }
  await assert.rejects(sign({ name: 'evm-short', payload: '0x00' }), { message: 'INVALID_INPUT' });
});

test('public key requests return verified public information', macOnly, async () => {
  const ed = await bridge.getSignerPublicKeyForTest({ name: 'sol-key' }, helper);
  assert.deepEqual(ed, { name: 'sol-key', curve: 'ed25519', publicKey: '0xd75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a', address: encodeBase58('0xd75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a'), });
  const secp = await bridge.getSignerPublicKeyForTest({ name: 'evm-key' }, helper);
  assert.equal(computeAddress(secp.publicKey), hardhatAddress);
  assert.deepEqual(starts('evm-key'), ['public-key evm-key']);
  await assert.rejects(bridge.getSignerPublicKeyForTest({ name: 'missing-key' }, helper), { message: 'NOT_FOUND' });
});

test('invalid signer requests fail before launching a helper', macOnly, async () => {
  for (const request of [
    { name: 'invalid', payload: '0x' },
    { name: 'invalid', payload: '00' },
    { name: 'invalid', payload: '0x0' },
    { name: 'invalid', payload: '0xzz' },
    { name: 'invalid', payload: `0x${'00'.repeat(1233)}` },
    { name: 'invalid\n', payload: '0x00' },
    { name: '', payload: '0x00' },
    { name: 'invalid', payload: '0x00', authTTL: -1 },
    { name: 'invalid', payload: '0x00', authTTL: 0.5 },
    { name: 'invalid', payload: '0x00', helperPath: '/tmp/helper' },
    { name: 'invalid', payload: 1 },
  ]) {
    await assert.rejects(sign(request as never), { message: 'Invalid signer request' });
  }
  await assert.rejects(bridge.getSignerPublicKeyForTest({ name: 'bad\n' }, helper), { message: 'Invalid signer request' });
  assert.equal(starts('invalid').length, 0);
});
