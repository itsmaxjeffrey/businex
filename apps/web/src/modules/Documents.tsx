import React, { useMemo, useState } from "react";
import { api } from "../lib/api";
import { useData } from "../lib/hooks";
import { useToast } from "../lib/store";
import { Spinner, EmptyState, formatDate } from "../components/ui";
import { IconPlus, IconSearch } from "../components/icons";

/** Inline formatting: bold, italics, inline code. */
function inlineFormat(s: string): string {
  let t = s;
  t = t.replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");
  t = t.replace(/\*(.+?)\*/g, "<em>$1</em>");
  t = t.replace(/`([^`]+)`/g, "<code>$1</code>");
  return t;
}

/** Tiny markdown renderer: headings, lists, paragraphs, inline formatting. */
function renderMarkdown(src: string): string {
  const esc = src.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const lines = esc.split("\n");
  const html: string[] = [];
  let listOpen = false;

  for (const raw of lines) {
    const line = raw.trim();
    if (line.startsWith("- ")) {
      if (!listOpen) { html.push("<ul>"); listOpen = true; }
      html.push("<li>" + inlineFormat(line.slice(2)) + "</li>");
      continue;
    }
    if (listOpen) { html.push("</ul>"); listOpen = false; }
    if (line.startsWith("### ")) html.push("<h3>" + inlineFormat(line.slice(4)) + "</h3>");
    else if (line.startsWith("## ")) html.push("<h2>" + inlineFormat(line.slice(3)) + "</h2>");
    else if (line.startsWith("# ")) html.push("<h1>" + inlineFormat(line.slice(2)) + "</h1>");
    else if (line === "") html.push("");
    else html.push("<p>" + inlineFormat(line) + "</p>");
  }
  if (listOpen) html.push("</ul>");
  return html.join("\n");
}

export function DocumentsModule() {
  const { data, loading, reload } = useData<{ items: any[] }>("/documents");
  const { push } = useToast();
  const [selected, setSelected] = useState<any | null>(null);
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState({ title: "", body: "" });

  const docs = useMemo(() => {
    const items = data?.items ?? [];
    if (!query.trim()) return items;
    const q = query.toLowerCase();
    return items.filter((d) => (d.title + " " + d.body).toLowerCase().includes(q));
  }, [data, query]);

  const create = async () => {
    try {
      const doc = await api.post<any>("/documents", { title: "Untitled document", body: "" });
      setSelected(doc);
      setDraft({ title: doc.title, body: doc.body });
      setEditing(true);
      reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  const save = async () => {
    if (!selected) return;
    try {
      const updated = await api.patch<any>("/documents/" + selected.id, draft);
      setSelected(updated);
      setEditing(false);
      push("Document saved", "success");
      reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  const remove = async () => {
    if (!selected) return;
    try {
      await api.del("/documents/" + selected.id);
      setSelected(null);
      push("Document deleted", "success");
      reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  return (
    <div className="flex h-full">
      <aside className="flex w-64 flex-col border-r" style={{ borderColor: "var(--panel-border)" }}>
        <div className="flex items-center gap-2 p-3">
          <div className="relative flex-1">
            <span className="absolute left-3 top-1/2 -translate-y-1/2 opacity-45"><IconSearch size={14} /></span>
            <input
              className="input"
              style={{ paddingLeft: 32, fontSize: "0.8rem" }}
              placeholder="Filter docs…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
          <button className="btn btn-primary" onClick={create}><IconPlus size={14} /></button>
        </div>
        <div className="flex-1 overflow-y-auto px-2 pb-3">
          {loading && <Spinner />}
          {docs.map((d) => (
            <button
              key={d.id}
              className="btn btn-ghost w-full justify-start"
              style={{ background: selected?.id === d.id ? "rgba(255,107,53,0.14)" : undefined }}
              onClick={() => { setSelected(d); setDraft({ title: d.title, body: d.body }); setEditing(false); }}
            >
              <span className="truncate text-left">{d.title}</span>
            </button>
          ))}
          {!loading && docs.length === 0 && <EmptyState title="No documents" hint="Your knowledge base lives here." />}
        </div>
      </aside>

      <div className="flex flex-1 flex-col overflow-hidden">
        {!selected ? (
          <EmptyState
            title="Select a document"
            hint="Or create one — content is indexed in full-text search and reachable by agents through the MCP tools."
            action={<button className="btn btn-primary" onClick={create}>New document</button>}
          />
        ) : (
          <>
            <div className="flex items-center justify-between border-b px-6 py-3" style={{ borderColor: "var(--panel-border)" }}>
              <div>
                {editing ? (
                  <input
                    className="input"
                    style={{ width: 420 }}
                    value={draft.title}
                    onChange={(e) => setDraft({ ...draft, title: e.target.value })}
                  />
                ) : (
                  <h2 className="font-display text-2xl">{selected.title}</h2>
                )}
                <div className="mt-1 text-xs opacity-45">Updated {formatDate(selected.updatedAt)}</div>
              </div>
              <div className="flex gap-2">
                {editing ? (
                  <>
                    <button className="btn" onClick={() => setEditing(false)}>Cancel</button>
                    <button className="btn btn-primary" onClick={save}>Save</button>
                  </>
                ) : (
                  <>
                    <button className="btn" onClick={() => setEditing(true)}>Edit</button>
                    <button className="btn btn-danger" onClick={remove}>Delete</button>
                  </>
                )}
              </div>
            </div>
            <div className="flex-1 overflow-y-auto">
              {editing ? (
                <textarea
                  className="textarea"
                  style={{
                    height: "100%", borderRadius: 0, border: "none", background: "transparent",
                    padding: "1.5rem", fontFamily: "var(--font-mono)", fontSize: "0.85rem", lineHeight: 1.7,
                  }}
                  value={draft.body}
                  onChange={(e) => setDraft({ ...draft, body: e.target.value })}
                  placeholder="# Start writing…"
                />
              ) : (
                <article
                  className="prose-docs px-8 py-6 text-[0.92rem] leading-relaxed"
                  dangerouslySetInnerHTML={{ __html: renderMarkdown(selected.body || "*Empty document.*") }}
                />
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
