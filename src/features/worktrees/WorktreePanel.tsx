/**
 * Worktree パネル（担当: WS-F）: タスクの worktree 状態（git status）とプロジェクト全体の worktree 一覧、タスク削除。
 * 仮実装: git status の表示のみ。
 */
import { useEffect, useState } from "react";
import { gitApi, toAppError } from "../../api";
import type { AppError, GitStatus } from "../../api";

export function WorktreePanel({ taskId }: { taskId: string }) {
  const [status, setStatus] = useState<GitStatus | null>(null);
  const [error, setError] = useState<AppError | null>(null);

  useEffect(() => {
    gitApi
      .getGitStatus(taskId)
      .then(setStatus)
      .catch((e) => setError(toAppError(e)));
  }, [taskId]);

  if (error) return <p className="error">{error.message}</p>;
  if (!status) return <p className="muted">読み込み中…</p>;
  return (
    <div>
      <div>
        {status.branch} {status.upstream && <span className="muted">→ {status.upstream}</span>}
      </div>
      <div className="muted">
        ahead {status.ahead} / behind {status.behind}
      </div>
    </div>
  );
}
