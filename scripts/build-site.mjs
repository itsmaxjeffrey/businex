#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const built = spawnSync("npm", ["run", "build", "-w", "@businex/web", "--", "--base=./"], { cwd: root, stdio: "inherit" });
if (built.status !== 0) process.exit(built.status ?? 1);
const source = path.join(root, "apps/web/dist");
const target = path.join(root, "site/app");
mkdirSync(target, { recursive: true });
// Keep prior fingerprinted chunks so an already-open demo can still load its modules.
for (const name of readdirSync(source)) cpSync(path.join(source, name), path.join(target, name), { recursive: true });
console.log("Published demo build staged in site/app");
