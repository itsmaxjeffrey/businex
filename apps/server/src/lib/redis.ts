import { randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import { createClient } from "redis";
import { z } from "zod";
import { bus, type EventBus, type BusinexEvent } from "../events";

const envelopeSchema = z.object({
  sender: z.string().uuid(),
  event: z.object({
    type: z.string().min(1).max(128),
    workspaceId: z.string().min(1).max(256),
    payload: z.unknown(),
    at: z.string().datetime(),
  }),
});

/** Redis carries transient notifications. Durable records remain in the database. */
export function createRedisBridge(eventBus: EventBus, options: {
  url?: string; password?: string; prefix?: string;
}) {
  const sender = randomUUID();
  const channel = `${options.prefix ?? "businex"}:workspace-events:v1`;
  let subscribed = false;
  let started = false;
  let stopped = false;
  let lastWarning = 0;
  const warn = () => {
    if (Date.now() - lastWarning > 30_000) {
      // Never log Redis errors: they can include connection strings or credentials.
      console.warn("[businex] Redis unavailable; live updates are local to this process");
      lastWarning = Date.now();
    }
  };
  const publisher = options.url ? createClient({
    url: options.url,
    password: options.password,
    disableOfflineQueue: true,
    socket: { connectTimeout: 3000, reconnectStrategy: retries => Math.min(250 * 2 ** Math.min(retries, 5), 8000) },
  }) : undefined;
  const subscriber = publisher?.duplicate();
  publisher?.on("error", warn);
  subscriber?.on("error", warn);

  const relay = (event: BusinexEvent) => {
    // PTYs belong to one process and must not be broadcast across instances.
    if (event.type.startsWith("terminal.") || !publisher?.isReady) return;
    try {
      const message = JSON.stringify({ sender, event });
      if (Buffer.byteLength(message) > 1_048_576) { warn(); return; }
      void publisher.publish(channel, message).catch(warn);
    } catch { warn(); }
  };
  return {
    status(): "disabled" | "connected" | "degraded" {
      if (!publisher) return "disabled";
      return publisher.isReady && subscriber?.isReady && subscribed ? "connected" : "degraded";
    },
    async start() {
      if (started || stopped || !publisher || !subscriber) return;
      started = true;
      eventBus.on("publish", relay);
      try {
        await Promise.all([publisher.connect(), subscriber.connect()]);
        if (stopped) return;
        await subscriber.subscribe(channel, message => {
          if (Buffer.byteLength(message) > 1_048_576) return;
          try {
            const parsed = envelopeSchema.safeParse(JSON.parse(message));
            if (!parsed.success || parsed.data.sender === sender || parsed.data.event.type.startsWith("terminal.")) return;
            // Remote deliveries never emit "publish", preventing loops and duplicates.
            eventBus.emit("event", parsed.data.event);
          } catch { /* Ignore invalid messages from the internal channel. */ }
        });
        subscribed = true;
      } catch { if (!stopped) warn(); }
    },
    stop() {
      stopped = true;
      eventBus.off("publish", relay);
      if (publisher?.isOpen) publisher.destroy();
      if (subscriber?.isOpen) subscriber.destroy();
      subscribed = false;
    },
  };
}

export const redisBridge = createRedisBridge(bus, {
  url: process.env.BUSINEX_REDIS_URL,
  password: process.env.BUSINEX_REDIS_PASSWORD_FILE
    ? readFileSync(process.env.BUSINEX_REDIS_PASSWORD_FILE, "utf8").trim()
    : undefined,
  prefix: process.env.BUSINEX_REDIS_PREFIX ?? "businex",
});
