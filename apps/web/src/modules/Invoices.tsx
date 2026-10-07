import React, { useState } from "react";
import { api } from "../lib/api";
import { useData } from "../lib/hooks";
import { useToast } from "../lib/store";
import { Modal, Field, ModalActions, Spinner, EmptyState, Badge, formatDate, formatMoney } from "../components/ui";
import { IconPlus } from "../components/icons";

const STATUS_TONE: Record<string, "neutral" | "ember" | "sage" | "rose" | "gold" | "sky"> = {
  draft: "neutral", sent: "sky", paid: "sage", overdue: "rose", void: "neutral",
};

export function InvoicesModule() {
  const { data, loading, reload } = useData<{ items: any[] }>("/invoices");
  const { push } = useToast();
  const [showModal, setShowModal] = useState(false);
  const [detail, setDetail] = useState<any | null>(null);
  const [form, setForm] = useState({
    issueDate: new Date().toISOString().slice(0, 10),
    dueDate: new Date(Date.now() + 30 * 86400000).toISOString().slice(0, 10),
    currency: "USD",
    taxRate: "0",
    notes: "",
  });
  const [items, setItems] = useState([{ description: "", quantity: "1", unitPrice: "0" }]);

  const subtotal = items.reduce((s, i) => s + (Number(i.quantity) || 0) * (Number(i.unitPrice) || 0), 0);
  const total = subtotal * (1 + (Number(form.taxRate) || 0));

  const create = async () => {
    try {
      await api.post("/invoices", {
        ...form,
        taxRate: Number(form.taxRate) || 0,
        items: items.filter((i) => i.description.trim()).map((i) => ({
          description: i.description,
          quantity: Number(i.quantity) || 1,
          unitPrice: Number(i.unitPrice) || 0,
        })),
      });
      push("Invoice created", "success");
      setShowModal(false);
      setItems([{ description: "", quantity: "1", unitPrice: "0" }]);
      reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  const openDetail = async (id: string) => {
    try {
      setDetail(await api.get<any>("/invoices/" + id));
    } catch (e: any) { push(e.message, "error"); }
  };

  const setStatus = async (id: string, status: string) => {
    try {
      await api.patch("/invoices/" + id, { status });
      push("Invoice marked " + status, "success");
      reload();
      if (detail?.id === id) setDetail(await api.get<any>("/invoices/" + id));
    } catch (e: any) { push(e.message, "error"); }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between px-6 pt-5">
        <div>
          <h2 className="font-display text-2xl">Invoices</h2>
          <p className="text-xs opacity-50">{data?.items.length ?? 0} invoices · print-ready documents</p>
        </div>
        <button className="btn btn-primary" onClick={() => setShowModal(true)}><IconPlus size={15} /> New invoice</button>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {loading ? <Spinner /> : (data?.items.length ?? 0) === 0 ? (
          <EmptyState title="No invoices yet" hint="Create invoices with line items, tax and due dates — agents can create them too via MCP." action={<button className="btn btn-primary" onClick={() => setShowModal(true)}>Create first invoice</button>} />
        ) : (
          <div className="card overflow-hidden">
            <table className="table">
              <thead><tr><th>Number</th><th>Status</th><th>Issue</th><th>Due</th><th>Total</th><th></th></tr></thead>
              <tbody>
                {data?.items.map((inv) => (
                  <tr key={inv.id}>
                    <td className="font-mono text-xs">{inv.number}</td>
                    <td><Badge tone={STATUS_TONE[inv.status] ?? "neutral"}>{inv.status}</Badge></td>
                    <td className="opacity-70">{formatDate(inv.issueDate)}</td>
                    <td className="opacity-70">{formatDate(inv.dueDate)}</td>
                    <td className="font-mono">{formatMoney(inv.total, inv.currency)}</td>
                    <td>
                      <div className="flex justify-end gap-2">
                        <button className="btn btn-ghost text-xs" onClick={() => openDetail(inv.id)}>Open</button>
                        {inv.status === "draft" && <button className="btn btn-ghost text-xs" onClick={() => setStatus(inv.id, "sent")}>Mark sent</button>}
                        {inv.status === "sent" && <button className="btn btn-ghost text-xs" onClick={() => setStatus(inv.id, "paid")}>Mark paid</button>}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {detail && (
        <Modal title={detail.number} onClose={() => setDetail(null)} wide>
          <div className="flex items-center justify-between">
            <Badge tone={STATUS_TONE[detail.status] ?? "neutral"}>{detail.status}</Badge>
            <a className="btn" href={"/api/invoices/" + detail.id + "/print"} target="_blank" rel="noreferrer">Print / PDF</a>
          </div>
          <div className="mt-4 grid grid-cols-3 gap-3 text-sm">
            <div><div className="label">Issue date</div>{formatDate(detail.issueDate)}</div>
            <div><div className="label">Due date</div>{formatDate(detail.dueDate)}</div>
            <div><div className="label">Currency</div>{detail.currency}</div>
          </div>
          <table className="table mt-5">
            <thead><tr><th>Item</th><th>Qty</th><th>Unit</th><th>Amount</th></tr></thead>
            <tbody>
              {(detail.items ?? []).map((i: any) => (
                <tr key={i.id}>
                  <td>{i.description}</td>
                  <td>{i.quantity}</td>
                  <td>{formatMoney(i.unitPrice, detail.currency)}</td>
                  <td className="font-mono">{formatMoney(i.amount, detail.currency)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="mt-4 flex justify-end gap-6 text-sm">
            <div className="opacity-60">Subtotal: <span className="font-mono">{formatMoney(detail.subtotal, detail.currency)}</span></div>
            <div className="opacity-60">Tax: <span className="font-mono">{(detail.taxRate * 100).toFixed(1)}%</span></div>
            <div className="font-display text-lg">Total: <span className="font-mono">{formatMoney(detail.total, detail.currency)}</span></div>
          </div>
          <ModalActions>
            <button className="btn" onClick={() => setDetail(null)}>Close</button>
          </ModalActions>
        </Modal>
      )}

      {showModal && (
        <Modal title="New invoice" onClose={() => setShowModal(false)} wide>
          <div className="grid grid-cols-3 gap-3">
            <Field label="Issue date"><input className="input" type="date" value={form.issueDate} onChange={(e) => setForm({ ...form, issueDate: e.target.value })} /></Field>
            <Field label="Due date"><input className="input" type="date" value={form.dueDate} onChange={(e) => setForm({ ...form, dueDate: e.target.value })} /></Field>
            <Field label="Tax rate"><input className="input" type="number" step="0.01" min="0" max="1" value={form.taxRate} onChange={(e) => setForm({ ...form, taxRate: e.target.value })} /></Field>
          </div>

          <div className="mt-5">
            <div className="flex items-center justify-between">
              <span className="label mb-0">Line items</span>
              <button className="btn btn-ghost text-xs" onClick={() => setItems([...items, { description: "", quantity: "1", unitPrice: "0" }])}>+ Add line</button>
            </div>
            <div className="mt-2 flex flex-col gap-2">
              {items.map((item, idx) => (
                <div key={idx} className="grid grid-cols-[1fr_80px_110px] gap-2">
                  <input className="input" placeholder="Description" value={item.description} onChange={(e) => setItems(items.map((it, i) => i === idx ? { ...it, description: e.target.value } : it))} />
                  <input className="input" type="number" value={item.quantity} onChange={(e) => setItems(items.map((it, i) => i === idx ? { ...it, quantity: e.target.value } : it))} />
                  <input className="input" type="number" value={item.unitPrice} onChange={(e) => setItems(items.map((it, i) => i === idx ? { ...it, unitPrice: e.target.value } : it))} />
                </div>
              ))}
            </div>
          </div>

          <Field label="Notes"><textarea className="textarea" value={form.notes} onChange={(e) => setForm({ ...form, notes: e.target.value })} /></Field>

          <div className="mt-4 flex justify-end gap-6 text-sm">
            <div className="opacity-60">Subtotal: <span className="font-mono">{formatMoney(subtotal, form.currency)}</span></div>
            <div className="font-display text-lg">Total: <span className="font-mono">{formatMoney(total, form.currency)}</span></div>
          </div>

          <ModalActions>
            <button className="btn" onClick={() => setShowModal(false)}>Cancel</button>
            <button className="btn btn-primary" onClick={create}>Create invoice</button>
          </ModalActions>
        </Modal>
      )}
    </div>
  );
}
