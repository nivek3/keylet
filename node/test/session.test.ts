import assert from 'node:assert/strict';
import { createPrivateKey, createPublicKey, sign, verify } from 'node:crypto';
import { spawn } from 'node:child_process';
import { chmodSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { privateKeyToAccount } from 'viem/accounts';
import { Wallet as Wallet5, utils } from 'ethers-v5';
import { Wallet as Wallet6, HDNodeWallet, verifyMessage } from 'ethers-v6';
import { getKeyPair } from '../src/index.js';

// Public Hardhat development wallet ONLY. Never fund these accounts.
const address = '0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266';
const secondAddress = '0x70997970c51812dc3a010c7d01b50e0d17dc79c8';
const thirdKey = HDNodeWallet.fromPhrase('test test test test test test test test test test test junk', '', "m/44'/60'/0'/0/2").privateKey;
const thirdAddress = '0x3c44cdddb6a900fa2b585dd299e03d12fa4293bc';
const secondKey = HDNodeWallet.fromPhrase('test test test test test test test test test test test junk', '', "m/44'/60'/0'/0/1").privateKey;
const folder = mkdtempSync(join(tmpdir(), 'keylet-public-test-'));
const helper = join(folder, 'mock.mjs');
writeFileSync(
  helper,
  `#!${process.execPath}
import { appendFileSync, writeSync } from 'node:fs';
import { createECDH, createPrivateKey, createPublicKey } from 'node:crypto';
import { createInterface } from 'node:readline';
appendFileSync(process.argv[1] + '.starts', process.pid + '\\n');
const ttl = Number(process.argv[3]) * 1000;
let deadline = 0;
for await (const input of createInterface({ input: process.stdin })) {
const request = JSON.parse(input);
if (!deadline || Date.now() >= deadline) {
  appendFileSync(process.argv[1] + '.auth', request.name + '\\n');
  deadline = Date.now() + ttl;
}
if (Object.keys(request).sort().join(',') !== 'curve,name,path' || typeof request.path !== 'string') {
  process.stderr.write('INVALID_INPUT\\n'); process.exit(1);
}
appendFileSync(process.argv[1] + '.calls', request.name + '\\n');
if (request.name.startsWith('slow')) await new Promise(resolve => setTimeout(resolve, 300));
if (request.name.startsWith('overflow-out')) { process.stdout.write('x'.repeat(2049)); process.exit(0); }
if (request.name.startsWith('overflow-err')) { process.stderr.write('x'.repeat(1025)); process.exit(0); }
if (request.name.startsWith('cancel')) { process.stderr.write('CANCELLED\\n'); process.exit(1); }
if (request.name.startsWith('missing')) { process.stderr.write('NOT_FOUND\\n'); process.exit(1); }
if (request.name.startsWith('authfail')) { process.stderr.write('AUTHENTICATION_FAILED\\n'); process.exit(1); }
if (request.name.startsWith('unknown')) { process.stderr.write('untrusted text containing a pretend secret'); process.exit(1); }
const second = request.path === "m/44'/60'/0'/0/1";
const third = request.path === "m/44'/60'/0'/0/2";
const hex = third ? '${thirdKey.slice(2)}' : second ? '${secondKey.slice(2)}' : 'ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
const ed = request.curve === 'ed25519';
const key = ed
  ? Buffer.from(request.name.startsWith('zero') ? '00'.repeat(32) : request.path.endsWith("/1'") ? '01'.repeat(32) : '9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60', 'hex')
  : Buffer.from(hex, 'hex');
let publicKey;
if (ed) {
  const privateKey = createPrivateKey({ key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), key]), format: 'der', type: 'pkcs8' });
  publicKey = createPublicKey(privateKey).export({ format: 'der', type: 'spki' }).subarray(-32);
} else {
  const ec = createECDH('secp256k1'); ec.setPrivateKey(key); publicKey = ec.getPublicKey(undefined, request.name.startsWith('uncompressed-public') ? 'uncompressed' : 'compressed');
}
if (request.name.startsWith('invalid-key')) key.fill(255);
writeSync(3, request.name.startsWith('short') ? key.subarray(1) : request.name.startsWith('overflow-key') ? Buffer.alloc(33) : key);
process.stdout.write(JSON.stringify({name:request.name,path:request.path,
  curve:request.name.startsWith('cross-curve') ? (ed ? 'secp256k1' : 'ed25519') : request.curve,
  publicKey:request.name.startsWith('wrong') ? 'invalid' : request.name.startsWith('mismatch') ? '0x' + '00'.repeat(ed ? 32 : 33) : '0x' + publicKey.toString('hex')}) + '\\n');
if (request.name.startsWith('short')) process.exit(0);
}
`
);
chmodSync(helper, 0o700);
const options = (name: string) => ({ helperPath: helper, name, curve: 'secp256k1' as const, path: "m/44'/60'/0'/0/0" });
const calls = (name: string) =>
  readFileSync(helper + '.calls', 'utf8')
    .split('\n')
    .filter((v) => v === name).length;
const macOnly = { skip: process.platform !== 'darwin' };

test('key reads share one helper; consumers use their own viem / ethers objects', macOnly, async () => {
  const explicit = options('concurrent');
  const { path: _path, ...defaultPath } = explicit;
  const [one, two, three] = await Promise.all([getKeyPair(explicit), getKeyPair(defaultPath), getKeyPair({ ...defaultPath, index: 0 })]);
  assert.equal(calls('concurrent'), 3);
  assert.deepEqual(Object.keys(one).sort(), ['privateKey', 'publicKey']);
  assert.match(one.privateKey, /^0x[0-9a-f]{64}$/);
  assert.match(one.publicKey, /^0x0[23][0-9a-f]{64}$/);
  assert.deepEqual(one, two);
  assert.deepEqual(one, three);
  const viem = privateKeyToAccount(one.privateKey);
  const v5 = new Wallet5(two.privateKey);
  const v6 = new Wallet6(three.privateKey);
  assert.equal(v6.signingKey.compressedPublicKey, one.publicKey);
  assert.equal(v5._signingKey().compressedPublicKey, one.publicKey);
  for (const account of [viem, v5, v6]) assert.equal(account.address.toLowerCase(), address);
  const message = 'public test only';
  assert.equal(utils.verifyMessage(message, await v5.signMessage(message)).toLowerCase(), address);
  assert.equal(verifyMessage(message, await v6.signMessage(message)).toLowerCase(), address);
  assert.equal(verifyMessage(message, await viem.signMessage({ message })).toLowerCase(), address);
});

test('index selects a different key pair from the same wallet', macOnly, async () => {
  const { path: _path, ...selection } = options('index-pairs');
  const first = await getKeyPair(selection);
  const second = await getKeyPair({ ...selection, index: 1 });
  assert.notEqual(first.publicKey, second.publicKey);
  assert.equal(new Wallet6(second.privateKey).address.toLowerCase(), secondAddress);
  assert.equal(new Wallet6(second.privateKey).signingKey.compressedPublicKey, second.publicKey);
  const third = await getKeyPair({ ...selection, index: 2 });
  const explicit = await getKeyPair({ ...selection, path: "m/44'/60'/0'/0/2" });
  assert.equal(new Wallet6(third.privateKey).address.toLowerCase(), thirdAddress);
  assert.deepEqual(third, explicit);
  assert.notEqual(third.publicKey, second.publicKey);
  assert.equal(calls('index-pairs'), 4);
});

test('failures end the session; only explicit subsequent calls retry', macOnly, async () => {
  for (const [name, code] of [
    ['cancel-once', 'CANCELLED'],
    ['missing-once', 'NOT_FOUND'],
    ['authfail-once', 'AUTHENTICATION_FAILED'],
    ['unknown-once', 'HELPER_FAILED'],
    ['short-once', 'INVALID_HELPER_RESPONSE'],
    ['wrong-once', 'INVALID_HELPER_RESPONSE'],
    ['uncompressed-public-once', 'INVALID_HELPER_RESPONSE'],
    ['invalid-key-once', 'INVALID_HELPER_RESPONSE'],
    ['cross-curve-once', 'INVALID_HELPER_RESPONSE'],
    ['mismatch-once', 'INVALID_HELPER_RESPONSE'],
    ['overflow-out-once', 'INVALID_HELPER_RESPONSE'],
    ['overflow-err-once', 'INVALID_HELPER_RESPONSE'],
    ['overflow-key-once', 'INVALID_HELPER_RESPONSE'],
  ]) {
    await assert.rejects(getKeyPair(options(name)), { message: code });
    await assert.rejects(getKeyPair(options(name)), { message: code });
    assert.equal(calls(name), 2);
  }
});

test('invalid public selection fails before launching a helper', async () => {
  for (const change of [{ path: 'm/0' }, { name: 'name\n' }, { path: "m/44'/60'/0'/0/0\n" }, { path: "m/44'/60'/2147483648'/0/0" }, { helperPath: 'relative' }, { index: 0 }, { curve: undefined }, { curve: 'p256' }]) {
    await assert.rejects(getKeyPair({ ...options('invalid'), ...change } as never), { message: 'Invalid wallet selection' });
  }
});

test('fresh business processes independently unlock', macOnly, async () => {
  const modulePath = fileURLToPath(new URL('../src/index.ts', import.meta.url));
  const source = `import { getKeyPair } from ${JSON.stringify(modulePath)};
    await getKeyPair(${JSON.stringify(options('separate-process'))});`;
  for (let i = 0; i < 2; i++) {
    await new Promise<void>((resolve, reject) => {
      const child = spawn(process.execPath, ['--import', 'tsx', '--input-type=module', '-e', source], { stdio: 'ignore' });
      child.on('error', reject);
      child.on('close', (code) => (code === 0 ? resolve() : reject(new Error('test child failed'))));
    });
  }
  assert.equal(calls('separate-process'), 2);
});

const edOptions = (name: string) => ({ helperPath: helper, name, curve: 'ed25519' as const, path: "m/44'/501'/0'/0'" });

test('Ed25519 returns a 32-byte seed and public key usable for offline signatures', macOnly, async () => {
  const one = await getKeyPair(edOptions('ed-pair'));
  assert.deepEqual(Object.keys(one).sort(), ['privateKey', 'publicKey']);
  assert.match(one.privateKey, /^0x[0-9a-f]{64}$/);
  assert.equal(one.publicKey, '0xd75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a');
  const privateKey = createPrivateKey({ key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), Buffer.from(one.privateKey.slice(2), 'hex')]), format: 'der', type: 'pkcs8' });
  const signature = sign(null, Buffer.alloc(0), privateKey);
  // RFC 8032 test vector 1, empty message; public test key only.
  assert.equal(signature.toString('hex'), 'e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b');
  assert.equal(verify(null, Buffer.alloc(0), createPublicKey(privateKey), signature), true);
});

test('curves and explicit Ed25519 paths return independently verified keys', macOnly, async () => {
  const name = 'both-curves';
  const [secp, ed, duplicate] = await Promise.all([getKeyPair(options(name)), getKeyPair(edOptions(name)), getKeyPair(edOptions(name))]);
  assert.notEqual(secp.publicKey, ed.publicKey);
  assert.deepEqual(ed, duplicate);
  assert.equal(calls(name), 3);
  const second = await getKeyPair({ ...edOptions(name), path: "m/44'/501'/0'/1'" });
  assert.notEqual(second.publicKey, ed.publicKey);
  assert.equal(calls(name), 4);
});

test('zero Ed25519 seeds are valid; malformed and mismatched helper responses fail closed', macOnly, async () => {
  const zero = await getKeyPair(edOptions('zero-ed'));
  assert.equal(zero.privateKey, '0x' + '00'.repeat(32));
  assert.match(zero.publicKey, /^0x[0-9a-f]{64}$/);
  for (const name of ['short-ed', 'wrong-ed', 'mismatch-ed', 'cross-curve-ed']) {
    await assert.rejects(getKeyPair(edOptions(name)), { message: 'INVALID_HELPER_RESPONSE' });
    await assert.rejects(getKeyPair(edOptions(name)), { message: 'INVALID_HELPER_RESPONSE' });
    assert.equal(calls(name), 2);
  }
});

test('Rust authentication waits leave Node timers and filesystem workers responsive', macOnly, async () => {
  const pending = Promise.all(Array.from({ length: 4 }, () => getKeyPair(options('slow-concurrent'))));
  const first = await Promise.race([pending.then(() => 'keys'), stat(helper).then(() => 'filesystem')]);
  assert.equal(first, 'filesystem');
  const keys = await pending;
  assert.equal(calls('slow-concurrent'), 4);
  for (const key of keys) assert.deepEqual(key, keys[0]);
});


test('authenticationTtl reuses only authentication, expires, and supports zero', macOnly, async () => {
  const auths = (name: string) => readFileSync(helper + '.auth', 'utf8').split('\n').filter(v => v === name).length;
  const selection = { ...options('ttl-session'), authenticationTtl: 1 };
  const one = await getKeyPair(selection);
  const two = await getKeyPair(selection);
  assert.deepEqual(one, two);
  assert.equal(calls('ttl-session'), 2);
  assert.equal(auths('ttl-session'), 1);
  await new Promise(resolve => setTimeout(resolve, 1100));
  await getKeyPair(selection);
  assert.equal(auths('ttl-session'), 2);
  await getKeyPair({ ...selection, authenticationTtl: 0 });
  await getKeyPair({ ...selection, authenticationTtl: 0 });
  assert.equal(auths('ttl-session'), 4);
  for (const authenticationTtl of [5400, 86400, Number.MAX_SAFE_INTEGER]) {
    const result = await getKeyPair({ ...selection, authenticationTtl });
    assert.deepEqual(result, one);
  }
});
