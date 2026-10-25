import assert from 'node:assert/strict';
import { test } from 'node:test';
import { getKeyPair } from '../src/index.js';

// Filesystem lookup and path resolution now have Rust unit tests; Node checks the public boundary.
test('Rust bridge rejects invalid public selection before helper discovery', { skip: process.platform !== 'darwin' }, async () => {
  const base = { name: 'test', curve: 'secp256k1', helperPath: '/definitely-missing-keylet-helper' };
  for (const change of [
    ...[-1, Number.MAX_SAFE_INTEGER + 1, 0.5, NaN, Infinity, '300', null].map(authenticationTtl => ({ authenticationTtl })),
    { curve: undefined }, { curve: 'p256' }, { curve: null },
    { name: 'bad\n' }, { name: '' }, { helperPath: 'relative' },
    { index: -1 }, { index: 0.5 }, { index: NaN }, { index: Infinity },
    { index: 2147483648 }, { index: '1' }, { index: null },
    { path: "m/44'/60'/0'/0/0", index: 0 }, { path: '' },
    { path: "m/44'/60'/0'/0/0\n" }, { path: "m/44'/60'/2147483648'/0/0" },
    { curve: 'ed25519' }, { curve: 'ed25519', path: "m/0'/1" },
    { curve: 'ed25519', path: "m/0'", index: 0 }, { curve: 'ed25519', path: "m/2147483648'" },
    { curve: 'ed25519', path: "m/0'\n" }, { path: 1 },
  ]) {
    await assert.rejects(getKeyPair({ ...base, ...change } as never), { message: 'Invalid wallet selection' });
  }
});
