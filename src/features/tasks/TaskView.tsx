/**
 * タスクタブの中身（担当: WS-F）。左 = チャット（WS-G）、右 = サイドパネル（PR / コンフリクト / worktree）。
 * 各パネルは taskId だけを受け取り、自分のストアから状態を読む（パネル間の props 依存を作らない）。
 */
import { useProjectStore } from "../../store/projectStore";
import { useTabStore, type SidePanelKind } from "../../store/tabStore";
import { ChatPanel } from "../chat/ChatPanel";
import { PrPanel } from "../pr/PrPanel";
import { ConflictPanel } from "../conflicts/ConflictPanel";
import { WorktreePanel } from "../worktrees/WorktreePanel";

const PANELS: { key: SidePanelKind; label: string }[] = [
  { key: "pr", label: "PR" },
  { key: "conflicts", label: "コンフリクト" },
  { key: "worktree", label: "Worktree" },
];

export function TaskView({ taskId }: { taskId: string }) {
  const task = useProjectStore((s) => s.findTask(taskId));
  const panel = useTabStore((s) => s.sidePanelByTask[taskId] ?? "pr");
  const setSidePanel = useTabStore((s) => s.setSidePanel);

  if (!task) return <div className="empty">タスクが見つかりません</div>;

  return (
    <div className="task-view">
      <ChatPanel taskId={taskId} />
      <aside className="side-panel">
        <div style={{ marginBottom: 8 }}>
          <div style={{ fontWeight: 600 }}>{task.title}</div>
          <div className="muted">
            {task.branch} ← {task.baseBranch} / {task.agent}
          </div>
        </div>
        <div style={{ display: "flex", gap: 4, marginBottom: 12 }}>
          {PANELS.map((p) => (
            <button key={p.key} disabled={panel === p.key} onClick={() => setSidePanel(taskId, p.key)}>
              {p.label}
            </button>
          ))}
        </div>
        {panel === "pr" && <PrPanel taskId={taskId} />}
        {panel === "conflicts" && <ConflictPanel taskId={taskId} />}
        {panel === "worktree" && <WorktreePanel taskId={taskId} />}
      </aside>
    </div>
  );
}
