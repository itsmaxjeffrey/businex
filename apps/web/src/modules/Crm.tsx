import React, { useState } from "react";
import { api } from "../lib/api";
import { useData } from "../lib/hooks";
import { useToast } from "../lib/store";
import { Modal, Field, ModalActions, Spinner, EmptyState, Badge, formatDate, formatMoney, Avatar } from "../components/ui";
import { IconPlus } from "../components/icons";

type Tab = "contacts" | "companies" | "deals";

export function CrmModule() {
  const [tab, setTab] = useState<Tab>("contacts");
  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b px-6 py-3" style={{ borderColor: "var(--panel-border)" }}>
        {(["contacts", "companies", "deals"] as Tab[]).map((t) => (
          <button
            key={t}
            className={"btn " + (tab === t ? "btn-primary" : "btn-ghost")}
            onClick={() => setTab(t)}
            style={{ textTransform: "capitalize" }}
          >
            {t}
          </button>
        ))}
      </div>
      <div className="flex-1 overflow-y-auto">
        {tab === "contacts" && <ContactsTab />}
        {tab === "companies" && <CompaniesTab />}
        {tab === "deals" && <DealsTab />}
      </div>
    </div>
  );
}

function ContactsTab() {
  const { data, loading, reload } = useData<{ items: any[] }>("/crm/contacts");
  const [showModal, setShowModal] = useState(false);
  const { push } = useToast();
  const [form, setForm] = useState({ firstName: "", lastName: "", email: "", title: "" });

  const create = async () => {
    try {
      await api.post("/crm/contacts", { ...form, email: form.email || null, title: form.title || null });
      push("Contact created", "success");
      setShowModal(false);
      setForm({ firstName: "", lastName: "", email: "", title: "" });
      reload();
    } catch (e: any) {
      push(e.message, "error");
    }
  };

  return (
    <div className="p-6">
      <div className="flex items-center justify-between">
        <h2 className="font-display text-2xl">Contacts</h2>
        <button className="btn btn-primary" onClick={() => setShowModal(true)}><IconPlus size={15} /> New contact</button>
      </div>

      {loading ? <Spinner /> : (data?.items.length ?? 0) === 0 ? (
        <EmptyState title="No contacts yet" hint="Add the people you work with — every record is searchable and agent-accessible." action={<button className="btn btn-primary" onClick={() => setShowModal(true)}>Add first contact</button>} />
      ) : (
        <div className="card mt-4 overflow-hidden">
          <table className="table">
            <thead><tr><th>Name</th><th>Email</th><th>Title</th><th>Added</th></tr></thead>
            <tbody>
              {data?.items.map((c) => (
                <tr key={c.id}>
                  <td>
                    <div className="flex items-center gap-2">
                      <Avatar name={c.firstName + " " + c.lastName} size={26} />
                      <span className="font-medium">{c.firstName} {c.lastName}</span>
                    </div>
                  </td>
                  <td className="opacity-70">{c.email ?? "—"}</td>
                  <td className="opacity-70">{c.title ?? "—"}</td>
                  <td className="opacity-50">{formatDate(c.createdAt)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {showModal && (
        <Modal title="New contact" onClose={() => setShowModal(false)}>
          <div className="flex flex-col gap-4">
            <div className="grid grid-cols-2 gap-3">
              <Field label="First name"><input className="input" value={form.firstName} onChange={(e) => setForm({ ...form, firstName: e.target.value })} /></Field>
              <Field label="Last name"><input className="input" value={form.lastName} onChange={(e) => setForm({ ...form, lastName: e.target.value })} /></Field>
            </div>
            <Field label="Email"><input className="input" type="email" value={form.email} onChange={(e) => setForm({ ...form, email: e.target.value })} /></Field>
            <Field label="Title"><input className="input" value={form.title} onChange={(e) => setForm({ ...form, title: e.target.value })} /></Field>
          </div>
          <ModalActions>
            <button className="btn" onClick={() => setShowModal(false)}>Cancel</button>
            <button className="btn btn-primary" onClick={create}>Create contact</button>
          </ModalActions>
        </Modal>
      )}
    </div>
  );
}

function CompaniesTab() {
  const { data, loading, reload } = useData<{ items: any[] }>("/crm/companies");
  const [showModal, setShowModal] = useState(false);
  const { push } = useToast();
  const [form, setForm] = useState({ name: "", domain: "", industry: "" });

  const create = async () => {
    try {
      await api.post("/crm/companies", { ...form, domain: form.domain || null, industry: form.industry || null });
      push("Company created", "success");
      setShowModal(false);
      setForm({ name: "", domain: "", industry: "" });
      reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  return (
    <div className="p-6">
      <div className="flex items-center justify-between">
        <h2 className="font-display text-2xl">Companies</h2>
        <button className="btn btn-primary" onClick={() => setShowModal(true)}><IconPlus size={15} /> New company</button>
      </div>
      {loading ? <Spinner /> : (data?.items.length ?? 0) === 0 ? (
        <EmptyState title="No companies yet" hint="Track the organizations you work with." action={<button className="btn btn-primary" onClick={() => setShowModal(true)}>Add first company</button>} />
      ) : (
        <div className="card mt-4 overflow-hidden">
          <table className="table">
            <thead><tr><th>Company</th><th>Domain</th><th>Industry</th><th>Added</th></tr></thead>
            <tbody>
              {data?.items.map((c) => (
                <tr key={c.id}>
                  <td className="font-medium">{c.name}</td>
                  <td className="opacity-70">{c.domain ?? "—"}</td>
                  <td><Badge tone="sky">{c.industry ?? "—"}</Badge></td>
                  <td className="opacity-50">{formatDate(c.createdAt)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {showModal && (
        <Modal title="New company" onClose={() => setShowModal(false)}>
          <div className="flex flex-col gap-4">
            <Field label="Name"><input className="input" value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} /></Field>
            <Field label="Domain"><input className="input" value={form.domain} onChange={(e) => setForm({ ...form, domain: e.target.value })} placeholder="acme.com" /></Field>
            <Field label="Industry"><input className="input" value={form.industry} onChange={(e) => setForm({ ...form, industry: e.target.value })} /></Field>
          </div>
          <ModalActions>
            <button className="btn" onClick={() => setShowModal(false)}>Cancel</button>
            <button className="btn btn-primary" onClick={create}>Create company</button>
          </ModalActions>
        </Modal>
      )}
    </div>
  );
}

const STAGES = ["lead", "qualified", "proposal", "negotiation", "won", "lost"];

function DealsTab() {
  const { data, loading, reload } = useData<{ items: any[] }>("/crm/deals");
  const [showModal, setShowModal] = useState(false);
  const { push } = useToast();
  const [form, setForm] = useState({ name: "", value: "0", stage: "lead", closeDate: "" });
  const [dragging, setDragging] = useState<string | null>(null);

  const create = async () => {
    try {
      await api.post("/crm/deals", {
        name: form.name,
        value: Number(form.value) || 0,
        stage: form.stage,
        closeDate: form.closeDate || null,
      });
      push("Deal created", "success");
      setShowModal(false);
      setForm({ name: "", value: "0", stage: "lead", closeDate: "" });
      reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  const moveDeal = async (dealId: string, stage: string) => {
    try {
      await api.patch("/crm/deals/" + dealId, { stage });
      reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  const deals = data?.items ?? [];

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between px-6 pt-6">
        <h2 className="font-display text-2xl">Deals pipeline</h2>
        <button className="btn btn-primary" onClick={() => setShowModal(true)}><IconPlus size={15} /> New deal</button>
      </div>
      {loading ? <Spinner /> : (
        <div className="flex flex-1 gap-3 overflow-x-auto p-6">
          {STAGES.map((stage) => {
            const stageDeals = deals.filter((d) => d.stage === stage);
            const total = stageDeals.reduce((s, d) => s + d.value, 0);
            return (
              <div
                key={stage}
                className="kanban-col"
                onDragOver={(e) => e.preventDefault()}
                onDrop={() => { if (dragging) moveDeal(dragging, stage); setDragging(null); }}
              >
                <div className="flex items-center justify-between px-1 pb-2">
                  <span className="text-xs font-semibold uppercase tracking-widest opacity-60">{stage}</span>
                  <span className="font-mono text-[11px] opacity-50">{formatMoney(total)}</span>
                </div>
                <div className="flex flex-col gap-2">
                  {stageDeals.map((d) => (
                    <div
                      key={d.id}
                      className="kanban-card"
                      draggable
                      onDragStart={() => setDragging(d.id)}
                      onDragEnd={() => setDragging(null)}
                    >
                      <div className="text-sm font-medium">{d.name}</div>
                      <div className="mt-1 flex items-center justify-between">
                        <span className="font-mono text-xs" style={{ color: "var(--color-gold-500)" }}>{formatMoney(d.value)}</span>
                        {d.closeDate && <span className="text-[11px] opacity-50">{formatDate(d.closeDate)}</span>}
                      </div>
                    </div>
                  ))}
                  {stageDeals.length === 0 && <div className="px-1 py-3 text-xs opacity-35">Drop deals here</div>}
                </div>
              </div>
            );
          })}
        </div>
      )}

      {showModal && (
        <Modal title="New deal" onClose={() => setShowModal(false)}>
          <div className="flex flex-col gap-4">
            <Field label="Deal name"><input className="input" value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} /></Field>
            <div className="grid grid-cols-2 gap-3">
              <Field label="Value"><input className="input" type="number" value={form.value} onChange={(e) => setForm({ ...form, value: e.target.value })} /></Field>
              <Field label="Stage">
                <select className="select" value={form.stage} onChange={(e) => setForm({ ...form, stage: e.target.value })}>
                  {STAGES.map((s) => <option key={s} value={s}>{s}</option>)}
                </select>
              </Field>
            </div>
            <Field label="Expected close"><input className="input" type="date" value={form.closeDate} onChange={(e) => setForm({ ...form, closeDate: e.target.value })} /></Field>
          </div>
          <ModalActions>
            <button className="btn" onClick={() => setShowModal(false)}>Cancel</button>
            <button className="btn btn-primary" onClick={create}>Create deal</button>
          </ModalActions>
        </Modal>
      )}
    </div>
  );
}
