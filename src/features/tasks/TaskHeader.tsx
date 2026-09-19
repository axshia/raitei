/** タスクタブ上部のヘッダー（担当: WS-F）: タイトル・エージェント・権限・ブランチ・worktree パスとメニュー */
import { useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { Task } from "../../api";
import { useProjectStore } from "../../store/projectStore";
import { useTabStore } from "../../store/tabStore";
import { Menu } from "../../components/layout/Menu";
import { Modal } from "../../components/layout/Modal";
import { DeleteTaskDialog } from "./DeleteTaskDialog";
import { shortenHome } from "./branch";

function RenameDialog({ task, onClose }: { task: Task; onClose(): void }) {
  const updateTask = useProjectStore((s) => s.updateTask);
  const [title, setTitle] = useState(task.title);
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    if (!title.trim()) return;
    setBusy(true);
    try {
      await updateTask({ taskId: task.id, title: title.trim() });
      onClose();
    } catch {
      setBusy(false);
    }
  };
  return (
    <Modal
      title="タイトルを変更"
      onClose={onClose}
      onSubmit={submit}
      busy={busy}
      footer={
        <>
          <button type="button" onClick={onClose} disabled={busy}>
            キャンセル
          </button>
          <button type="submit" className="primary" disabled={!title.trim() || busy}>
            変更
          </button>
        </>
      }
    >
      <input value={title} onChange={(e) => setTitle(e.target.value)} />
    </Modal>
  );
}

export function TaskHeader({ task }: { task: Task }) {
  const project = useProjectStore((s) => s.findProject(task.projectId));
  const updateTask = useProjectStore((s) => s.updateTask);
  const setSubView = useTabStore((s) => s.setSubView);
  const [dialog, setDialog] = useState<"delete" | "rename" | null>(null);

  const reveal = () => void revealItemInDir(task.worktreePath).catch(() => {});
  const copyPath = () => void navigator.clipboard?.writeText(task.worktreePath).catch(() => {});

  return (
    <header className="task-header">
      <div className="task-header-main">
        <div className="task-header-title">
          <span className="truncate" title={task.title}>
            {task.title}
          </span>
          <span className={`badge ${task.agent}`}>{task.agent}</span>
          <span
            className={`badge ${task.permission === "full" ? "warn" : ""}`}
            title={task.permission === "full" ? "フル権限（承認・サンドボックスなし）" : "安全（編集のみ自動承認 / workspace-write）"}
          >
            {task.permission === "full" ? "full" : "safe"}
          </span>
          {task.prNumber != null && (
            <button className="badge accent badge-button" onClick={() => setSubView(task.id, "pr")} title="PR を表示">
              PR #{task.prNumber}
            </button>
          )}
        </div>
        <div className="task-header-meta mono">
          {project && <span className="muted">{project.name}</span>}
          <span className="sep">·</span>
          <span className="selectable" title="作業ブランチ">
            {task.branch}
          </span>
          <span className="subtle">←</span>
          <span className="muted" title="base ブランチ">
            {task.baseBranch}
          </span>
          <span className="sep">·</span>
          <span className="subtle truncate path-link" title={`${task.worktreePath}\nクリックで Finder に表示`} onClick={reveal}>
            {shortenHome(task.worktreePath)}
          </span>
        </div>
      </div>
      <Menu
        label="タスクのメニュー"
        trigger={() => (
          <button className="ghost icon" title="タスクのメニュー">
            ⋯
          </button>
        )}
        items={[
          { label: "タイトルを変更…", onSelect: () => setDialog("rename") },
          {
            label: task.permission === "full" ? "権限を「安全」に変更" : "権限を「フル」に変更",
            hint: "次回の送信から",
            onSelect: () =>
              void updateTask({ taskId: task.id, permission: task.permission === "full" ? "safe" : "full" }).catch(() => {}),
          },
          "separator",
          { label: "worktree を Finder で表示", onSelect: reveal },
          { label: "worktree のパスをコピー", onSelect: copyPath },
          "separator",
          { label: "タスクを削除…", danger: true, onSelect: () => setDialog("delete") },
        ]}
      />
      {dialog === "delete" && <DeleteTaskDialog task={task} onClose={() => setDialog(null)} />}
      {dialog === "rename" && <RenameDialog task={task} onClose={() => setDialog(null)} />}
    </header>
  );
}
