import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { getKeyPair } from 'keylet';

test('release addon has no test path override', { skip: process.platform !== 'darwin' }, () => {
  const bridge = createRequire(import.meta.url)('../dist/keylet.node');
  assert.equal('getKeyPairForTest' in bridge, false);
});

// Bundle discovery has Rust unit tests; Node checks the public boundary.
test('Rust bridge rejects invalid public selection before helper discovery', { skip: process.platform !== 'darwin' }, async () => {
  const base = { name: 'test', curve: 'secp256k1' };
  for (const change of [
    ...[-1, Number.MAX_SAFE_INTEGER + 1, 0.5, NaN, Infinity, '300', null].map((authTTL) => ({ authTTL })),
    { authenticationTtl: 300 },
    { curve: undefined },
    { curve: 'p256' },
    { curve: null },
    { name: 'bad\n' },
    { name: '' },
    { helperPath: 'relative' },
    { helperPath: '/tmp/helper' },
    { unknown: true },
    { index: -1 },
    { index: 0.5 },
    { index: NaN },
    { index: Infinity },
    { index: 2147483648 },
    { index: '1' },
    { index: null },
    { path: "m/44'/60'/0'/0/0", index: 0 },
    { path: '' },
    { path: "m/44'/60'/0'/0/0\n" },
    { path: "m/44'/60'/2147483648'/0/0" },
    { curve: 'ed25519' },
    { curve: 'ed25519', path: "m/0'/1" },
    { curve: 'ed25519', path: "m/0'", index: 0 },
    { curve: 'ed25519', path: "m/2147483648'" },
    { curve: 'ed25519', path: "m/0'\n" },
    { path: 1 },
  ]) {
    await assert.rejects(getKeyPair({ ...base, ...change } as never), { message: 'Invalid wallet selection' });
  }
});
