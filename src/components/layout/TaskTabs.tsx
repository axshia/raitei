/**
 * タスクタブバー（担当: WS-F）。
 * 実行中インジケータは agentStore.runningByTask を読むだけ（書き込まない）。
 * 中クリックでも閉じられる。ショートカット: ⌘1〜9 で n 番目、⌘⇧[ / ⌘⇧] で左右（App.tsx で登録）。
 */
import { useProjectStore } from "../../store/projectStore";
import { useTabStore } from "../../store/tabStore";
import { useAgentStore } from "../../store/agentStore";

function Tab({ taskId, index }: { taskId: string; index: number }) {
  const task = useProjectStore((s) => s.findTask(taskId));
  const project = useProjectStore((s) => (task ? s.findProject(task.projectId) : undefined));
  const active = useTabStore((s) => s.activeTaskId === taskId);
  const activateTask = useTabStore((s) => s.activateTask);
  const closeTask = useTabStore((s) => s.closeTask);
  const running = useAgentStore((s) => !!s.runningByTask[taskId]);

  const title = task
    ? `${task.title}\n${project?.name ?? ""} · ${task.branch} ← ${task.baseBranch}\n${task.agent} / ${task.permission}${index < 9 ? `\n⌘${index + 1}` : ""}`
    : taskId;

  return (
    <div
      role="tab"
      aria-selected={active}
      className={`task-tab ${active ? "active" : ""}`}
      title={title}
      onClick={() => activateTask(taskId)}
      onMouseDown={(e) => {
        if (e.button === 1) {
          e.preventDefault();
          closeTask(taskId);
        }
      }}
    >
      <span className={`dot ${running ? "running" : ""} agent-${task?.agent ?? "none"}`} />
      <span className="task-tab-title truncate">{task?.title ?? "（不明なタスク）"}</span>
      {project && <span className="task-tab-project truncate">{project.name}</span>}
      <button
        className="ghost icon task-tab-close"
        aria-label="タブを閉じる"
        onClick={(e) => {
          e.stopPropagation();
          closeTask(taskId);
        }}
      >
        ×
      </button>
    </div>
  );
}

export function TaskTabs() {
  const openTaskIds = useTabStore((s) => s.openTaskIds);
  return (
    <nav className="task-tabs" role="tablist">
      {openTaskIds.map((id, i) => (
        <Tab key={id} taskId={id} index={i} />
      ))}
      <div className="task-tabs-fill" />
    </nav>
  );
}
