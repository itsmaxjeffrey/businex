import { createHmac, randomBytes, timingSafeEqual, createHash } from "node:crypto";
import { config } from "../config";

/** Signed session token: <id>.<hmac>. The id is stored hashed server-side. */
export function signToken(id: string): string {
  const sig = createHmac("sha256", config.secret).update(id).digest("base64url");
  return id + "." + sig;
}

export function verifyToken(token: string): string | null {
  const dot = token.lastIndexOf(".");
  if (dot <= 0) return null;
  const id = token.slice(0, dot);
  const sig = token.slice(dot + 1);
  const expected = createHmac("sha256", config.secret).update(id).digest("base64url");
  const a = Buffer.from(sig), b = Buffer.from(expected);
  if (a.length !== b.length || !timingSafeEqual(a, b)) return null;
  return id;
}

export function hashToken(token: string): string {
  return createHash("sha256").update(token).digest("hex");
}

export function randomToken(bytes = 24): string {
  return randomBytes(bytes).toString("base64url");
}
