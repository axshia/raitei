/**
 * コンフリクト解消パネル（担当: WS-H）:
 * base 取り込み → 競合ファイル一覧 → ファイルごとに ours/theirs/解決済み or AI に依頼 → コミット・push / 中止。
 * 仮実装: 状態表示と base 取り込みボタンのみ。
 */
import { useEffect } from "react";
import { usePrStore } from "../../store/prStore";

export function ConflictPanel({ taskId }: { taskId: string }) {
  const state = usePrStore((s) => s.conflictByTask[taskId]);
  const error = usePrStore((s) => s.errorByTask[taskId]);
  const { refreshConflicts, startBaseMerge } = usePrStore();

  useEffect(() => {
    void refreshConflicts(taskId);
  }, [taskId, refreshConflicts]);

  return (
    <div>
      <button onClick={() => startBaseMerge(taskId)}>base を取り込む</button>
      {error && <p className="error">{error.message}</p>}
      {state && (
        <ul>
          {state.files.map((f) => (
            <li key={f.path}>
              {f.path} <span className="muted">({f.kind})</span>
            </li>
          ))}
        </ul>
      )}
      {state && !state.mergeInProgress && <p className="muted">マージ中ではありません</p>}
    </div>
  );
}
