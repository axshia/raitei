/** タスク削除ダイアログ（担当: WS-F）: worktree 削除 / ブランチ削除 / 強制 を選んで `delete_task` */
import { useState } from "react";
import type { Task } from "../../api";
import { useProjectStore } from "../../store/projectStore";
import { useAgentStore } from "../../store/agentStore";
import { ConfirmDialog } from "../../components/layout/ConfirmDialog";
import { shortenHome } from "./branch";

export function DeleteTaskDialog({ task, onClose }: { task: Task; onClose(): void }) {
  const deleteTask = useProjectStore((s) => s.deleteTask);
  const running = useAgentStore((s) => !!s.runningByTask[task.id]);
  const [removeWorktree, setRemoveWorktree] = useState(true);
  const [deleteBranch, setDeleteBranch] = useState(false);
  const [force, setForce] = useState(false);

  return (
    <ConfirmDialog
      title={`タスク「${task.title}」を削除`}
      confirmLabel="削除"
      danger
      onClose={onClose}
      onConfirm={async () => {
        try {
          await deleteTask(task, { removeWorktree, deleteBranch: removeWorktree && deleteBranch, force });
        } finally {
          useProjectStore.getState().clearError();
        }
      }}
    >
      <div className="check-list">
        <label className="check">
          <input type="checkbox" checked={removeWorktree} onChange={(e) => setRemoveWorktree(e.target.checked)} />
          <span>
            worktree を削除
            <span className="hint mono block">{shortenHome(task.worktreePath)}</span>
          </span>
        </label>
        <label className={`check ${removeWorktree ? "" : "disabled"}`}>
          <input
            type="checkbox"
            disabled={!removeWorktree}
            checked={removeWorktree && deleteBranch}
            onChange={(e) => setDeleteBranch(e.target.checked)}
          />
          <span>
            ローカルブランチを削除 <span className="mono">{task.branch}</span>
            <span className="hint block">リモートブランチは削除しません</span>
          </span>
        </label>
        <label className="check">
          <input type="checkbox" checked={force} onChange={(e) => setForce(e.target.checked)} />
          <span>
            強制（未コミットの変更・未マージのブランチも削除）
            <span className="hint block">worktree remove --force / branch -D</span>
          </span>
        </label>
      </div>
      {!removeWorktree && <div className="hint">worktree とブランチは残り、raitei の一覧と会話履歴だけを削除します。</div>}
      {running && (
        <div className="banner warn">
          <span className="grow">エージェントが実行中です。先に停止してください。</span>
        </div>
      )}
    </ConfirmDialog>
  );
}
