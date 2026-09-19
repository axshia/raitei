/**
 * PR パネル（担当: WS-H）: PR 未作成なら作成フォーム、作成済みなら状態（CI / レビュー / mergeable / コンフリクト）とマージ操作。
 * 仮実装: 状態取得と生 JSON 表示のみ。
 */
import { useEffect } from "react";
import { usePrStore } from "../../store/prStore";

export function PrPanel({ taskId }: { taskId: string }) {
  const pr = usePrStore((s) => s.prByTask[taskId]);
  const loading = usePrStore((s) => !!s.loadingByTask[taskId]);
  const error = usePrStore((s) => s.errorByTask[taskId]);
  const refreshPr = usePrStore((s) => s.refreshPr);

  useEffect(() => {
    void refreshPr(taskId);
  }, [taskId, refreshPr]);

  return (
    <div>
      <button disabled={loading} onClick={() => refreshPr(taskId)}>
        更新
      </button>
      {error && <p className="error">{error.message}</p>}
      {pr === null && <p className="muted">PR はまだありません（作成フォームは WS-H で実装）</p>}
      {pr && <pre style={{ whiteSpace: "pre-wrap" }}>{JSON.stringify(pr, null, 2)}</pre>}
    </div>
  );
}
