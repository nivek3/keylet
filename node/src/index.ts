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

export interface SignRequest {
  /** A local signer created with `keylet signer create` or `keylet signer derive`. */
  name: string;
  /**
   * Bytes to sign blindly. Ed25519: 1 to 1232 bytes, such as a serialized Solana transaction message.
   * secp256k1: exactly one 32-byte digest, such as an Ethereum transaction hash or a Bitcoin sighash.
   * Strings must be `0x`-prefixed hex.
   */
  payload: Uint8Array | string;
  /**
   * Optional non-negative safe integer seconds, capped by the app's **Reuse signer auth** setting.
   * Omit to follow the app; 0 authenticates every signature. Calls with the same name and authTTL share one session.
   */
  authTTL?: number;
}

export interface SignerPublicKey {
  name: string;
  curve: Curve;
  /** secp256k1: 33-byte compressed key. Ed25519: 32-byte public key. */
  publicKey: Hex;
  /** Ed25519 only: the Base58 public key, which is also the Solana address. */
  address?: string;
}

export interface SignerSignature extends SignerPublicKey {
  /** 64 bytes. secp256k1: `r || s`, low-S (RFC 6979). Ed25519: the standard signature. */
  signature: Hex;
  /** secp256k1 only: 0 or 1. Ethereum `v` is `27 + recoveryId`, or EIP-155 / typed-transaction parity. */
  recoveryId?: 0 | 1;
}

type NativeBridge = {
  getKeyPair(selection: WalletSelection): Promise<KeyPair>;
  signWithSigner(request: { name: string; payload: string; authTTL?: number }): Promise<SignerSignature>;
  getSignerPublicKey(request: { name: string }): Promise<SignerPublicKey>;
};
const require = createRequire(import.meta.url);
let bridge: NativeBridge | undefined;

function loadBridge(): NativeBridge {
  if (!isMainThread) throw new Error('Use the wallet in the main Node thread');
  if (os.platform() !== 'darwin') throw new Error('macOS is required');
  if (!bridge) {
    try {
      bridge = require('./keylet.node') as NativeBridge;
    } catch (cause) {
      throw new Error('KEYLET_BRIDGE_NOT_BUILT', { cause });
    }
  }
  return bridge;
}

async function call<T>(invalid: string, request: () => Promise<T>): Promise<T> {
  try {
    return await request();
  } catch (error) {
    if (error && typeof error === 'object' && 'code' in error && error.code === 'InvalidArg') {
      throw new Error(invalid);
    }
    throw error;
  }
}

export async function getKeyPair(selection: WalletSelection): Promise<KeyPair> {
  const native = loadBridge();
  return call('Invalid wallet selection', () => native.getKeyPair(selection));
}

function payloadHex(payload: unknown): unknown {
  return payload instanceof Uint8Array ? `0x${Buffer.from(payload.buffer, payload.byteOffset, payload.byteLength).toString('hex')}` : payload;
}

/**
 * Signs with a local signer kept in the app. The private key never leaves the app.
 * Keylet signs blindly: it does not decode the payload or apply transaction policies.
 * Every signature is verified against the returned public key before it is returned.
 */
export async function signWithSigner(request: SignRequest): Promise<SignerSignature> {
  const native = loadBridge();
  const input = request && typeof request === 'object' ? { ...request, payload: payloadHex(request.payload) } : request;
  return call('Invalid signer request', () => native.signWithSigner(input as never));
}

/** Reads a local signer's public key. The app asks for macOS authentication. */
export async function getSignerPublicKey(name: string): Promise<SignerPublicKey> {
  const native = loadBridge();
  return call('Invalid signer request', () => native.getSignerPublicKey({ name }));
}
