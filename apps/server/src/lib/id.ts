import { randomBytes } from "node:crypto";

const ALPHABET = "0123456789abcdefghijklmnopqrstuvwxyz";

function randomBase36(bytes: number): string {
  const buf = randomBytes(bytes);
  let out = "";
  for (const b of buf) out += ALPHABET[b % 36];
  return out;
}

/** Compact, sortable-ish, collision-resistant id with a type prefix. */
export function newId(prefix: string): string {
  return prefix + "_" + randomBase36(8) + randomBase36(4);
}
