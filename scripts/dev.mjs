#!/usr/bin/env node
// Runs the Businex server and web dev server together with one command.
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const children = [];

function run(name, command, args, cwd, color) {
  const child = spawn(command, args, {
    cwd,
    env: { ...process.env, FORCE_COLOR: "1" },
    stdio: ["ignore", "pipe", "pipe"],
    shell: false,
  });
  const prefix = "\x1b[" + color + "m[" + name + "]\x1b[0m ";
  const pipe = (stream) => {
    let buffer = "";
    stream.on("data", (chunk) => {
      buffer += chunk.toString();
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";
      for (const line of lines) console.log(prefix + line);
    });
  };
  pipe(child.stdout);
  pipe(child.stderr);
  child.on("exit", (code) => {
    console.log(prefix + "exited with code " + code);
    shutdown(code ?? 0);
  });
  children.push(child);
  return child;
}

function shutdown(code) {
  for (const child of children) {
    try { child.kill("SIGTERM"); } catch { /* already gone */ }
  }
  process.exit(code);
}

process.on("SIGINT", () => shutdown(0));
process.on("SIGTERM", () => shutdown(0));

const serverPort = process.env.BUSINEX_PORT ?? "8788";
const webPort = process.env.BUSINEX_WEB_PORT ?? "5199";

console.log("Businex dev — server :" + serverPort + " · web :" + webPort);
run("server", "npx", ["tsx", "watch", "src/index.ts"], path.join(root, "apps/server"), "33");
run("web", "npx", ["vite", "--port", webPort], path.join(root, "apps/web"), "36");
