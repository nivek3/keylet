import { createRequire } from 'node:module';
import * as os from 'node:os';
import { isMainThread } from 'node:worker_threads';

export type Hex = `0x${string}`;
export type Curve = 'secp256k1' | 'ed25519';
export type WalletSelection = {
  /** Optional non-negative safe integer seconds, capped by the app setting. Omit to follow the app; 0 authenticates every call. */
  authTTL?: number;
  name: string;
} & (
  | ({ curve: 'secp256k1' } & ({ index?: number; path?: never } | { path: string; index?: never }))
  /** SLIP-0010: callers choose the full hardened derivation path. */
  | { curve: 'ed25519'; path: string; index?: never }
);

export interface KeyPair {
  /** secp256k1: 33-byte compressed key (0x02/0x03 prefix). Ed25519: 32-byte public key. */
  publicKey: Hex;
  /** 32-byte secp256k1 scalar or Ed25519 seed; keep in memory and never log it. */
  privateKey: Hex;
}

type NativeBridge = { getKeyPair(selection: WalletSelection): Promise<KeyPair> };
const require = createRequire(import.meta.url);
let bridge: NativeBridge | undefined;

export async function getKeyPair(selection: WalletSelection): Promise<KeyPair> {
  if (!isMainThread) throw new Error('Use the wallet in the main Node thread');
  if (os.platform() !== 'darwin') throw new Error('macOS is required');
  if (!bridge) {
    try {
      bridge = require('./keylet.node') as NativeBridge;
    } catch (cause) {
      throw new Error('KEYLET_BRIDGE_NOT_BUILT', { cause });
    }
  }
  try {
    return await bridge.getKeyPair(selection);
  } catch (error) {
    if (error && typeof error === 'object' && 'code' in error && error.code === 'InvalidArg') {
      throw new Error('Invalid wallet selection');
    }
    throw error;
  }
}
