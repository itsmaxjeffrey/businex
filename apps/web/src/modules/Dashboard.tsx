import React from "react";
import { useData } from "../lib/hooks";
import { StatCard, Spinner, EmptyState, Badge, formatDate, formatMoney, timeAgo } from "../components/ui";

interface Overview {
  counts: Record<string, number>;
  pipeline: Array<{ stage: string; count: number; value: number }>;
  tasksByStatus: Array<{ status: string; count: number }>;
  overdueTasks: number;
  openInvoices: { count: number; total: number };
  upcomingEvents: Array<{ id: string; title: string; startsAt: string; color: string }>;
  recentActivity: Array<{ action: string; entityType: string; entityId: string; actorType: string; createdAt: string }>;
}

const stageTone: Record<string, string> = {
  lead: "var(--color-ink-300)", qualified: "var(--color-sky-500)", proposal: "var(--color-gold-500)",
  negotiation: "var(--color-ember-500)", won: "var(--color-sage-500)", lost: "var(--color-rose-500)",
};

export function DashboardModule() {
  const { data, loading } = useData<Overview>("/analytics/overview");

  if (loading || !data) return <Spinner label="Loading mission control…" />;

  const maxValue = Math.max(1, ...data.pipeline.map((p) => p.value));

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="stagger grid grid-cols-2 gap-4 lg:grid-cols-4">
        <StatCard label="Contacts" value={data.counts.contacts} sub={data.counts.companies + " companies"} />
        <StatCard label="Open pipeline" value={formatMoney(data.pipeline.filter((p) => !["won", "lost"].includes(p.stage)).reduce((s, p) => s + p.value, 0))} sub={data.pipeline.reduce((s, p) => s + p.count, 0) + " deals"} accent="var(--color-gold-500)" />
        <StatCard label="Overdue tasks" value={data.overdueTasks} sub={data.tasksByStatus.reduce((s, t) => s + t.count, 0) + " tasks total"} accent={data.overdueTasks > 0 ? "var(--color-rose-500)" : "var(--color-sage-500)"} />
        <StatCard label="Open invoices" value={formatMoney(data.openInvoices.total)} sub={data.openInvoices.count + " awaiting payment"} accent="var(--color-sky-500)" />
      </div>

      <div className="mt-6 grid grid-cols-1 gap-4 lg:grid-cols-[1.3fr_1fr]">
        {/* Pipeline */}
        <div className="card">
          <div className="flex items-center justify-between">
            <h3 className="font-display text-lg">Deal pipeline</h3>
            <span className="text-xs opacity-50">value per stage</span>
          </div>
          <div className="mt-4 flex flex-col gap-3">
            {data.pipeline.length === 0 && <EmptyState title="No deals yet" hint="Open CRM and add your first deal to see the pipeline." />}
            {data.pipeline.map((p) => (
              <div key={p.stage}>
                <div className="flex items-center justify-between text-xs">
                  <span className="capitalize opacity-80">{p.stage}</span>
                  <span className="font-mono opacity-60">{formatMoney(p.value)} · {p.count}</span>
                </div>
                <div className="mt-1 h-2.5 overflow-hidden rounded-full" style={{ background: "rgba(242,237,228,0.07)" }}>
                  <div
                    className="h-full rounded-full transition-all"
                    style={{ width: Math.max(3, (p.value / maxValue) * 100) + "%", background: stageTone[p.stage] ?? "var(--color-ember-500)" }}
                  />
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Tasks + events */}
        <div className="flex flex-col gap-4">
          <div className="card">
            <h3 className="font-display text-lg">Work in flight</h3>
            <div className="mt-3 flex flex-wrap gap-2">
              {data.tasksByStatus.length === 0 && <span className="text-sm opacity-50">No tasks yet.</span>}
              {data.tasksByStatus.map((t) => (
                <Badge key={t.status} tone={t.status === "done" ? "sage" : t.status === "in_progress" ? "ember" : "neutral"}>
                  {t.status.replace("_", " ")} · {t.count}
                </Badge>
              ))}
            </div>
          </div>

          <div className="card">
            <h3 className="font-display text-lg">Upcoming</h3>
            <div className="mt-3 flex flex-col gap-2">
              {data.upcomingEvents.length === 0 && <span className="text-sm opacity-50">Nothing scheduled.</span>}
              {data.upcomingEvents.map((e) => (
                <div key={e.id} className="flex items-center gap-3 text-sm">
                  <span className="h-2.5 w-2.5 rounded-full" style={{ background: e.color }} />
                  <span className="flex-1 truncate">{e.title}</span>
                  <span className="text-xs opacity-55">{formatDate(e.startsAt)}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* Activity feed */}
      <div className="card mt-4">
        <h3 className="font-display text-lg">Recent activity</h3>
        <div className="mt-3 flex flex-col gap-2">
          {data.recentActivity.length === 0 && <span className="text-sm opacity-50">Activity will appear here as the workspace is used.</span>}
          {data.recentActivity.map((a, i) => (
            <div key={i} className="flex items-center gap-3 text-sm">
              <Badge tone={a.actorType === "agent" ? "ember" : "neutral"}>{a.actorType}</Badge>
              <span className="font-mono text-xs opacity-75">{a.action}</span>
              <span className="opacity-50">{a.entityType}</span>
              <span className="ml-auto text-xs opacity-45">{timeAgo(a.createdAt)}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
