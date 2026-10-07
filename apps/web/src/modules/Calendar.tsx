import React, { useMemo, useState } from "react";
import { api } from "../lib/api";
import { useData } from "../lib/hooks";
import { useToast } from "../lib/store";
import { Modal, Field, ModalActions, Spinner, EmptyState, formatDate } from "../components/ui";
import { IconPlus } from "../components/icons";

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

export function CalendarModule() {
  const [cursor, setCursor] = useState(() => new Date());
  const { data, loading, reload } = useData<{ items: any[] }>("/calendar/events");
  const { push } = useToast();
  const [showModal, setShowModal] = useState(false);
  const [form, setForm] = useState({ title: "", date: "", time: "09:00", duration: "1", description: "" });

  const monthLabel = cursor.toLocaleDateString(undefined, { month: "long", year: "numeric" });

  const grid = useMemo(() => {
    const first = new Date(cursor.getFullYear(), cursor.getMonth(), 1);
    const startOffset = (first.getDay() + 6) % 7; // Monday-first
    const start = new Date(first);
    start.setDate(first.getDate() - startOffset);
    const days: Date[] = [];
    for (let i = 0; i < 42; i++) {
      const d = new Date(start);
      d.setDate(start.getDate() + i);
      days.push(d);
    }
    return days;
  }, [cursor]);

  const eventsByDay = useMemo(() => {
    const map: Record<string, any[]> = {};
    for (const e of data?.items ?? []) {
      const key = e.startsAt.slice(0, 10);
      (map[key] ??= []).push(e);
    }
    return map;
  }, [data]);

  const create = async () => {
    try {
      const startsAt = new Date(form.date + "T" + form.time);
      const endsAt = new Date(startsAt.getTime() + Number(form.duration) * 3600000);
      await api.post("/calendar/events", {
        title: form.title,
        description: form.description || null,
        startsAt: startsAt.toISOString(),
        endsAt: endsAt.toISOString(),
      });
      push("Event created", "success");
      setShowModal(false);
      setForm({ title: "", date: "", time: "09:00", duration: "1", description: "" });
      reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  const today = new Date().toISOString().slice(0, 10);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between px-6 pt-5">
        <div className="flex items-center gap-3">
          <h2 className="font-display text-2xl">{monthLabel}</h2>
          <div className="flex gap-1">
            <button className="btn btn-ghost" onClick={() => setCursor(new Date(cursor.getFullYear(), cursor.getMonth() - 1, 1))}>←</button>
            <button className="btn btn-ghost" onClick={() => setCursor(new Date())}>Today</button>
            <button className="btn btn-ghost" onClick={() => setCursor(new Date(cursor.getFullYear(), cursor.getMonth() + 1, 1))}>→</button>
          </div>
        </div>
        <button className="btn btn-primary" onClick={() => setShowModal(true)}><IconPlus size={15} /> New event</button>
      </div>

      {loading ? <Spinner /> : (
        <div className="flex flex-1 flex-col p-6">
          <div className="grid grid-cols-7 gap-1 text-center">
            {WEEKDAYS.map((d) => <div key={d} className="label mb-1">{d}</div>)}
          </div>
          <div className="grid flex-1 grid-cols-7 gap-1">
            {grid.map((day) => {
              const key = day.toISOString().slice(0, 10);
              const inMonth = day.getMonth() === cursor.getMonth();
              const isToday = key === today;
              const dayEvents = eventsByDay[key] ?? [];
              return (
                <div
                  key={key}
                  className="overflow-hidden rounded-lg p-1.5"
                  style={{
                    background: isToday ? "rgba(255,107,53,0.12)" : "rgba(242,237,228,0.025)",
                    border: "1px solid " + (isToday ? "rgba(255,107,53,0.35)" : "var(--panel-border)"),
                    opacity: inMonth ? 1 : 0.35,
                    minHeight: 74,
                  }}
                >
                  <div className="text-[11px] font-semibold opacity-60">{day.getDate()}</div>
                  <div className="flex flex-col gap-0.5">
                    {dayEvents.slice(0, 2).map((e) => (
                      <div key={e.id} className="truncate rounded px-1 text-[10px]" style={{ background: e.color + "33", color: e.color }}>
                        {e.title}
                      </div>
                    ))}
                    {dayEvents.length > 2 && <div className="text-[9px] opacity-50">+{dayEvents.length - 2} more</div>}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {showModal && (
        <Modal title="New event" onClose={() => setShowModal(false)}>
          <div className="flex flex-col gap-4">
            <Field label="Title"><input className="input" value={form.title} onChange={(e) => setForm({ ...form, title: e.target.value })} /></Field>
            <div className="grid grid-cols-3 gap-3">
              <Field label="Date"><input className="input" type="date" value={form.date} onChange={(e) => setForm({ ...form, date: e.target.value })} /></Field>
              <Field label="Time"><input className="input" type="time" value={form.time} onChange={(e) => setForm({ ...form, time: e.target.value })} /></Field>
              <Field label="Hours"><input className="input" type="number" min="0.5" step="0.5" value={form.duration} onChange={(e) => setForm({ ...form, duration: e.target.value })} /></Field>
            </div>
            <Field label="Description"><textarea className="textarea" value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} /></Field>
          </div>
          <ModalActions>
            <button className="btn" onClick={() => setShowModal(false)}>Cancel</button>
            <button className="btn btn-primary" onClick={create}>Create event</button>
          </ModalActions>
        </Modal>
      )}
    </div>
  );
}
