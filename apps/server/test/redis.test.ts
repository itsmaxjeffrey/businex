import { afterAll, describe, expect, it } from "vitest";
import { createClient } from "redis";
import { randomUUID } from "node:crypto";
import { mkdtempSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { EventBus } from "../src/events";

const dataDir = mkdtempSync(path.join(os.tmpdir(), "businex-redis-test-"));
process.env.BUSINEX_DATA_DIR = dataDir;
afterAll(() => rmSync(dataDir, { recursive: true, force: true }));

describe.runIf(Boolean(process.env.BUSINEX_TEST_REDIS_URL))("Redis realtime integration", () => {
  it("relays exactly once, isolates installations and rejects invalid or terminal events", async () => {
    const { createRedisBridge } = await import("../src/lib/redis");
    const prefix = `test:${randomUUID()}`;
    const first = new EventBus(), second = new EventBus(), isolated = new EventBus();
    const options = { url: process.env.BUSINEX_TEST_REDIS_URL, prefix };
    const bridges = [createRedisBridge(first, options), createRedisBridge(second, options),
      createRedisBridge(isolated, { ...options, prefix: prefix + ":other" })];
    const received: unknown[][] = [[], [], []];
    [first, second, isolated].forEach((bus, i) => bus.on("event", e => received[i].push(e)));
    const raw = createClient({ url: options.url });
    raw.on("error", () => {});
    try {
      await Promise.all(bridges.map(b => b.start()));
      expect(bridges.every(b => b.status() === "connected")).toBe(true);
      const delivered = new Promise<void>(resolve => second.once("event", () => resolve()));
      first.publish("channel.message", "workspace-a", { message: "hello" });
      await Promise.race([delivered, new Promise((_, reject) => setTimeout(() => reject(new Error("Relay timed out")), 2000))]);
      expect(received[1][0]).toMatchObject({ workspaceId: "workspace-a", payload: { message: "hello" } });
      first.publish("terminal.data", "workspace-a", { terminalId: "local", data: "private" });
      await raw.connect();
      await raw.publish(`${prefix}:workspace-events:v1`, "invalid JSON");
      await raw.publish(`${prefix}:workspace-events:v1`, JSON.stringify({ sender: randomUUID(), event: { type: "channel.message" } }));
      await raw.publish(`${prefix}:workspace-events:v1`, JSON.stringify({ sender: randomUUID(), event: {
        type: "terminal.data", workspaceId: "workspace-a", payload: {}, at: new Date().toISOString(),
      } }));
      await new Promise(resolve => setTimeout(resolve, 100));
      expect(received.map(e => e.length)).toEqual([2, 1, 0]);
    } finally {
      bridges.forEach(b => b.stop());
      if (raw.isOpen) raw.destroy();
    }
    expect(bridges.every(b => b.status() === "degraded")).toBe(true);
  }, 10_000);
});

it("keeps local events available with Redis disabled", async () => {
  const { createRedisBridge } = await import("../src/lib/redis");
  const local = new EventBus();
  const bridge = createRedisBridge(local, {});
  const events: unknown[] = [];
  local.on("event", e => events.push(e));
  await bridge.start();
  local.publish("task.created", "workspace-a", {});
  expect(events).toHaveLength(1);
  expect(bridge.status()).toBe("disabled");
  bridge.stop();
});
