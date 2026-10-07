import React, { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { useData } from "../lib/hooks";
import { useSession, useToast } from "../lib/store";
import { realtime } from "../lib/ws";
import { Spinner, EmptyState, Avatar, timeAgo, Badge } from "../components/ui";
import { IconPlus, IconSend } from "../components/icons";

export function ChannelsModule() {
  const { data: channels, loading, reload } = useData<{ items: any[] }>("/channels");
  const { user } = useSession();
  const { push } = useToast();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [messages, setMessages] = useState<any[]>([]);
  const [body, setBody] = useState("");
  const [thread, setThread] = useState<any | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState("");
  const bottomRef = useRef<HTMLDivElement>(null);

  const selected = channels?.items.find((c) => c.id === selectedId) ?? null;

  const loadMessages = async (channelId: string) => {
    try {
      const res = await api.get<{ items: any[] }>("/channels/" + channelId + "/messages?limit=100");
      setMessages(res.items);
    } catch (e: any) { push(e.message, "error"); }
  };

  useEffect(() => {
    if (!selectedId) return;
    void loadMessages(selectedId);
  }, [selectedId]);

  // Realtime: append incoming messages for the open channel.
  useEffect(() => {
    return realtime.onEvent((event) => {
      if (event.type === "channel.message" && event.payload?.channelId === selectedId) {
        setMessages((m) => [...m, event.payload.message]);
      }
    });
  }, [selectedId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages.length]);

  const send = async () => {
    if (!body.trim() || !selectedId) return;
    try {
      const msg = await api.post<any>("/channels/" + selectedId + "/messages", {
        body,
        threadId: thread?.id ?? null,
      });
      setMessages((m) => [...m, msg]);
      setBody("");
    } catch (e: any) { push(e.message, "error"); }
  };

  const createChannel = async () => {
    try {
      const ch = await api.post<any>("/channels", { kind: "channel", name: newName.replace(/^#/, "") });
      setShowCreate(false);
      setNewName("");
      setSelectedId(ch.id);
      reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  const toTask = async (message: any) => {
    try {
      await api.post("/messages/" + message.id + "/task", { title: message.body.slice(0, 120) });
      push("Task created from message", "success");
    } catch (e: any) { push(e.message, "error"); }
  };

  return (
    <div className="flex h-full">
      <aside className="flex w-56 flex-col border-r" style={{ borderColor: "var(--panel-border)" }}>
        <div className="flex items-center justify-between p-3">
          <span className="label mb-0">Channels</span>
          <button className="btn btn-ghost" onClick={() => setShowCreate(true)}><IconPlus size={14} /></button>
        </div>
        <div className="flex-1 overflow-y-auto px-2 pb-3">
          {loading && <Spinner />}
          {channels?.items.map((c) => (
            <button
              key={c.id}
              className="btn btn-ghost w-full justify-start"
              style={{ background: selectedId === c.id ? "rgba(255,107,53,0.14)" : undefined }}
              onClick={() => { setSelectedId(c.id); setThread(null); }}
            >
              <span className="opacity-50">{c.kind === "dm" ? "◉" : "#"}</span>
              <span className="truncate">{c.name}</span>
            </button>
          ))}
        </div>
        <div className="border-t p-3 text-[11px] leading-relaxed opacity-50" style={{ borderColor: "var(--panel-border)" }}>
          Mention an agent with <span className="font-mono">@name</span> to dispatch work.
          Threads keep the channel clean.
        </div>
      </aside>

      <div className="flex flex-1 flex-col">
        {!selected ? (
          <EmptyState title="Pick a channel" hint="open-tag style channels where humans and agents work as one team." />
        ) : (
          <>
            <div className="flex items-center justify-between border-b px-5 py-3" style={{ borderColor: "var(--panel-border)" }}>
              <div>
                <h2 className="font-display text-xl">#{selected.name}</h2>
                {selected.topic && <div className="text-xs opacity-50">{selected.topic}</div>}
              </div>
              {thread && (
                <button className="btn btn-ghost text-xs" onClick={() => setThread(null)}>
                  ✕ Close thread ({thread.body.slice(0, 30)}…)
                </button>
              )}
            </div>

            <div className="flex-1 overflow-y-auto px-5 py-4">
              {messages.length === 0 && <EmptyState title="No messages yet" hint="Say hello, or mention an agent to start work." />}
              <div className="flex flex-col gap-3">
                {messages.map((m) => (
                  <div key={m.id} className={"group flex gap-3 " + (m.threadId && thread ? "ml-8" : "")}>
                    <Avatar
                      name={m.authorType === "agent" ? "Agent" : (user?.name ?? "User")}
                      color={m.authorType === "agent" ? "var(--color-ember-500)" : user?.avatarColor}
                      size={32}
                    />
                    <div className="flex-1">
                      <div className="flex items-center gap-2 text-xs">
                        <span className="font-semibold">{m.authorType === "agent" ? "Agent" : (user?.name ?? "You")}</span>
                        {m.authorType === "agent" && <Badge tone="ember">agent teammate</Badge>}
                        <span className="opacity-40">{timeAgo(m.createdAt)}</span>
                        <div className="ml-auto flex gap-1 opacity-0 transition-opacity group-hover:opacity-100">
                          <button className="btn btn-ghost text-[11px]" onClick={() => setThread(m)}>Reply in thread</button>
                          <button className="btn btn-ghost text-[11px]" onClick={() => toTask(m)}>→ Task</button>
                        </div>
                      </div>
                      <div className="mt-0.5 text-sm leading-relaxed opacity-90">{m.body}</div>
                    </div>
                  </div>
                ))}
                <div ref={bottomRef} />
              </div>
            </div>

            <div className="border-t p-4" style={{ borderColor: "var(--panel-border)" }}>
              {thread && (
                <div className="mb-2 text-xs opacity-55">
                  Replying in thread to: <span className="italic">{thread.body.slice(0, 70)}</span>
                </div>
              )}
              <div className="flex gap-2">
                <input
                  className="input"
                  placeholder={"Message #" + selected.name + " — try @agent to dispatch work"}
                  value={body}
                  onChange={(e) => setBody(e.target.value)}
                  onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); void send(); } }}
                />
                <button className="btn btn-primary" onClick={send}><IconSend size={15} /></button>
              </div>
            </div>
          </>
        )}
      </div>

      {showCreate && (
        <div className="fixed inset-0 z-[9000] flex items-center justify-center" onMouseDown={() => setShowCreate(false)}>
          <div className="absolute inset-0" style={{ background: "rgba(6,7,9,0.55)" }} />
          <div className="panel relative rounded-2xl p-6" style={{ width: 420 }} onMouseDown={(e) => e.stopPropagation()}>
            <h2 className="font-display text-xl">New channel</h2>
            <input className="input mt-4" placeholder="channel-name" value={newName} onChange={(e) => setNewName(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") void createChannel(); }} />
            <div className="mt-5 flex justify-end gap-2">
              <button className="btn" onClick={() => setShowCreate(false)}>Cancel</button>
              <button className="btn btn-primary" onClick={createChannel}>Create</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
