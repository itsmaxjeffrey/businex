import React from "react";

export function Avatar({ name, color, size = 30 }: { name: string; color?: string; size?: number }) {
  const initials = name.split(" ").map((p) => p[0]).slice(0, 2).join("").toUpperCase();
  return (
    <span
      className="inline-flex items-center justify-center rounded-full font-semibold"
      style={{
        width: size, height: size, fontSize: size * 0.38,
        background: (color ?? "#6366f1") + "33",
        color: color ?? "#6366f1",
        border: "1px solid " + (color ?? "#6366f1") + "55",
      }}
    >
      {initials || "?"}
    </span>
  );
}

export function EmptyState({ title, hint, action }: { title: string; hint?: string; action?: React.ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-14 text-center">
      <div
        className="mb-1 h-14 w-14 rounded-2xl"
        style={{ background: "linear-gradient(135deg, rgba(255,107,53,0.25), rgba(255,107,53,0.05))", border: "1px dashed rgba(255,107,53,0.4)" }}
      />
      <div className="font-display text-xl">{title}</div>
      {hint && <div className="max-w-sm text-sm opacity-60">{hint}</div>}
      {action}
    </div>
  );
}

export function Spinner({ label }: { label?: string }) {
  return (
    <div className="flex items-center gap-3 py-10 justify-center opacity-70">
      <span
        className="inline-block h-4 w-4 animate-spin rounded-full"
        style={{ border: "2px solid rgba(242,237,228,0.25)", borderTopColor: "var(--color-ember-500)" }}
      />
      {label && <span className="text-sm">{label}</span>}
    </div>
  );
}

export function Modal({ title, onClose, children, wide }: { title: string; onClose: () => void; children: React.ReactNode; wide?: boolean }) {
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-[9000] flex items-center justify-center p-6" onMouseDown={onClose}>
      <div className="absolute inset-0" style={{ background: "rgba(6,7,9,0.55)", backdropFilter: "blur(4px)" }} />
      <div
        className="panel fade-in relative rounded-2xl p-6"
        style={{ width: wide ? 720 : 460, maxHeight: "84vh", overflowY: "auto" }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="font-display text-xl">{title}</h2>
          <button className="btn btn-ghost" onClick={onClose}>✕</button>
        </div>
        {children}
      </div>
    </div>
  );
}

export function StatCard({ label, value, sub, accent }: { label: string; value: React.ReactNode; sub?: string; accent?: string }) {
  return (
    <div className="card relative overflow-hidden">
      <div className="label">{label}</div>
      <div className="font-display text-3xl" style={{ color: accent ?? "inherit" }}>{value}</div>
      {sub && <div className="mt-1 text-xs opacity-55">{sub}</div>}
      <div
        className="absolute -right-6 -top-6 h-20 w-20 rounded-full"
        style={{ background: "radial-gradient(circle, " + (accent ?? "var(--color-ember-500)") + "22, transparent 70%)" }}
      />
    </div>
  );
}

export function Badge({ children, tone = "neutral" }: { children: React.ReactNode; tone?: "neutral" | "ember" | "sage" | "rose" | "gold" | "sky" }) {
  const tones: Record<string, string> = {
    neutral: "rgba(242,237,228,0.08)",
    ember: "rgba(255,107,53,0.16)",
    sage: "rgba(127,183,126,0.16)",
    rose: "rgba(224,82,99,0.16)",
    gold: "rgba(227,178,60,0.16)",
    sky: "rgba(111,168,220,0.16)",
  };
  const colors: Record<string, string> = {
    neutral: "inherit", ember: "var(--color-ember-400)", sage: "var(--color-sage-500)",
    rose: "var(--color-rose-500)", gold: "var(--color-gold-500)", sky: "var(--color-sky-500)",
  };
  return <span className="chip" style={{ background: tones[tone], color: colors[tone] }}>{children}</span>;
}

export function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="label">{label}</span>
      {children}
    </label>
  );
}

export function ModalActions({ children }: { children: React.ReactNode }) {
  return <div className="mt-6 flex items-center justify-end gap-2">{children}</div>;
}

export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

export function formatMoney(amount: number, currency = "USD"): string {
  return new Intl.NumberFormat(undefined, { style: "currency", currency }).format(amount);
}

export function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return mins + "m ago";
  const hours = Math.floor(mins / 60);
  if (hours < 24) return hours + "h ago";
  return Math.floor(hours / 24) + "d ago";
}
