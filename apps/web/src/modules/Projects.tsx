import React, { useState } from "react";
import { api } from "../lib/api";
import { useData } from "../lib/hooks";
import { useToast } from "../lib/store";
import { Modal, Field, ModalActions, Spinner, EmptyState, Badge, formatDate } from "../components/ui";
import { IconPlus } from "../components/icons";

const COLUMNS = [
  { status: "backlog", label: "Backlog", tone: "neutral" as const },
  { status: "todo", label: "To do", tone: "sky" as const },
  { status: "in_progress", label: "In progress", tone: "ember" as const },
  { status: "review", label: "Review", tone: "gold" as const },
  { status: "done", label: "Done", tone: "sage" as const },
];

const PRIORITY_TONE: Record<string, "neutral" | "ember" | "rose" | "gold"> = {
  low: "neutral", medium: "gold", high: "ember", urgent: "rose",
};

export function ProjectsModule() {
  const projects = useData<{ items: any[] }>("/projects");
  const tasks = useData<{ items: any[] }>("/projects/tasks");
  const { push } = useToast();
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [showProject, setShowProject] = useState(false);
  const [showTask, setShowTask] = useState(false);
  const [dragging, setDragging] = useState<string | null>(null);
  const [projectForm, setProjectForm] = useState({ name: "" });
  const [taskForm, setTaskForm] = useState({ title: "", priority: "medium", dueDate: "" });

  const createProject = async () => {
    try {
      await api.post("/projects", { name: projectForm.name });
      push("Project created", "success");
      setShowProject(false);
      setProjectForm({ name: "" });
      projects.reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  const createTask = async () => {
    try {
      await api.post("/projects/tasks", {
        title: taskForm.title,
        priority: taskForm.priority,
        projectId: selectedProject,
        dueDate: taskForm.dueDate || null,
      });
      push("Task created", "success");
      setShowTask(false);
      setTaskForm({ title: "", priority: "medium", dueDate: "" });
      tasks.reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  const moveTask = async (taskId: string, status: string) => {
    try {
      await api.post("/projects/tasks/" + taskId + "/move", { status, position: Date.now() % 100000 });
      tasks.reload();
    } catch (e: any) { push(e.message, "error"); }
  };

  const visibleTasks = (tasks.data?.items ?? []).filter((t) => !selectedProject || t.projectId === selectedProject);

  return (
    <div className="flex h-full">
      {/* Project sidebar */}
      <aside className="flex w-56 flex-col border-r p-4" style={{ borderColor: "var(--panel-border)" }}>
        <div className="flex items-center justify-between">
          <h3 className="label mb-0">Projects</h3>
          <button className="btn btn-ghost" onClick={() => setShowProject(true)}><IconPlus size={14} /></button>
        </div>
        <div className="mt-3 flex flex-col gap-1 overflow-y-auto">
          <button
            className="btn btn-ghost justify-start"
            style={{ background: selectedProject === null ? "rgba(255,107,53,0.14)" : undefined }}
            onClick={() => setSelectedProject(null)}
          >
            All tasks
          </button>
          {projects.data?.items.map((p) => (
            <button
              key={p.id}
              className="btn btn-ghost justify-start"
              style={{ background: selectedProject === p.id ? "rgba(255,107,53,0.14)" : undefined }}
              onClick={() => setSelectedProject(p.id)}
            >
              <span className="h-2 w-2 rounded-full" style={{ background: p.color }} />
              <span className="truncate">{p.name}</span>
            </button>
          ))}
        </div>
      </aside>

      {/* Board */}
      <div className="flex flex-1 flex-col">
        <div className="flex items-center justify-between px-6 pt-5">
          <div>
            <h2 className="font-display text-2xl">
              {selectedProject ? projects.data?.items.find((p) => p.id === selectedProject)?.name : "All tasks"}
            </h2>
            <p className="text-xs opacity-50">{visibleTasks.length} tasks · drag between columns to update status</p>
          </div>
          <button className="btn btn-primary" onClick={() => setShowTask(true)}><IconPlus size={15} /> New task</button>
        </div>

        {tasks.loading ? <Spinner /> : (
          <div className="flex flex-1 gap-3 overflow-x-auto p-6">
            {COLUMNS.map((col) => {
              const colTasks = visibleTasks.filter((t) => t.status === col.status);
              return (
                <div
                  key={col.status}
                  className="kanban-col"
                  onDragOver={(e) => e.preventDefault()}
                  onDrop={() => { if (dragging) moveTask(dragging, col.status); setDragging(null); }}
                >
                  <div className="flex items-center justify-between px-1 pb-2">
                    <Badge tone={col.tone}>{col.label}</Badge>
                    <span className="font-mono text-[11px] opacity-50">{colTasks.length}</span>
                  </div>
                  <div className="flex flex-col gap-2">
                    {colTasks.map((t) => (
                      <div
                        key={t.id}
                        className="kanban-card"
                        draggable
                        onDragStart={() => setDragging(t.id)}
                        onDragEnd={() => setDragging(null)}
                      >
                        <div className="text-sm font-medium">{t.title}</div>
                        <div className="mt-2 flex items-center gap-2">
                          <Badge tone={PRIORITY_TONE[t.priority] ?? "neutral"}>{t.priority}</Badge>
                          {t.dueDate && <span className="text-[11px] opacity-50">{formatDate(t.dueDate)}</span>}
                        </div>
                      </div>
                    ))}
                    {colTasks.length === 0 && <div className="px-1 py-3 text-xs opacity-35">Drop tasks here</div>}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {showProject && (
        <Modal title="New project" onClose={() => setShowProject(false)}>
          <Field label="Project name"><input className="input" value={projectForm.name} onChange={(e) => setProjectForm({ name: e.target.value })} /></Field>
          <ModalActions>
            <button className="btn" onClick={() => setShowProject(false)}>Cancel</button>
            <button className="btn btn-primary" onClick={createProject}>Create project</button>
          </ModalActions>
        </Modal>
      )}

      {showTask && (
        <Modal title="New task" onClose={() => setShowTask(false)}>
          <div className="flex flex-col gap-4">
            <Field label="Title"><input className="input" value={taskForm.title} onChange={(e) => setTaskForm({ ...taskForm, title: e.target.value })} /></Field>
            <div className="grid grid-cols-2 gap-3">
              <Field label="Priority">
                <select className="select" value={taskForm.priority} onChange={(e) => setTaskForm({ ...taskForm, priority: e.target.value })}>
                  <option value="low">low</option><option value="medium">medium</option>
                  <option value="high">high</option><option value="urgent">urgent</option>
                </select>
              </Field>
              <Field label="Due date"><input className="input" type="date" value={taskForm.dueDate} onChange={(e) => setTaskForm({ ...taskForm, dueDate: e.target.value })} /></Field>
            </div>
          </div>
          <ModalActions>
            <button className="btn" onClick={() => setShowTask(false)}>Cancel</button>
            <button className="btn btn-primary" onClick={createTask}>Create task</button>
          </ModalActions>
        </Modal>
      )}
    </div>
  );
}
