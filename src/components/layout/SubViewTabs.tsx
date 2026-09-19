/**
 * タスクタブ内のサブビュー切替（担当: WS-F）: チャット / PR / コンフリクト / 変更。
 * バッジ用に agentStore / prStore の状態を読むだけ（書き込まない）。ショートカット: ⌃1〜4。
 */
import type { ReactNode } from "react";
import { useTabStore, DEFAULT_SUB_VIEW, type TaskSubView } from "../../store/tabStore";
import { useAgentStore } from "../../store/agentStore";
import { usePrStore } from "../../store/prStore";
import { useProjectStore } from "../../store/projectStore";

export const SUB_VIEWS: { key: TaskSubView; label: string }[] = [
  { key: "chat", label: "チャット" },
  { key: "pr", label: "PR" },
  { key: "conflicts", label: "コンフリクト" },
  { key: "changes", label: "変更" },
];

function Badge({ taskId, view }: { taskId: string; view: TaskSubView }): ReactNode {
  const running = useAgentStore((s) => !!s.runningByTask[taskId]);
  const prNumber = useProjectStore((s) => s.findTask(taskId)?.prNumber ?? null);
  const pr = usePrStore((s) => s.prByTask[taskId]);
  const conflict = usePrStore((s) => s.conflictByTask[taskId]);

  switch (view) {
    case "chat":
      return running ? <span className="dot running" /> : null;
    case "pr": {
      const n = pr?.number ?? prNumber;
      if (!n) return null;
      const cls = pr?.state === "merged" ? "accent" : pr?.hasConflicts || (pr?.checks.failed ?? 0) > 0 ? "danger" : "";
      return <span className={`badge ${cls}`}>#{n}</span>;
    }
    case "conflicts": {
      if (conflict?.mergeInProgress && conflict.files.length > 0)
        return <span className="badge danger">{conflict.files.length}</span>;
      if (pr?.hasConflicts) return <span className="dot danger" />;
      if (conflict?.mergeInProgress) return <span className="dot warn" />;
      return null;
    }
    default:
      return null;
  }
}

export function SubViewTabs({ taskId, right }: { taskId: string; right?: ReactNode }) {
  const current = useTabStore((s) => s.subViewByTask[taskId] ?? DEFAULT_SUB_VIEW);
  const setSubView = useTabStore((s) => s.setSubView);

  return (
    <div className="subview-tabs" role="tablist">
      {SUB_VIEWS.map((v, i) => (
        <button
          key={v.key}
          role="tab"
          aria-selected={current === v.key}
          className={`subview-tab ${current === v.key ? "active" : ""}`}
          title={`⌃${i + 1}`}
          onClick={() => setSubView(taskId, v.key)}
        >
          {v.label}
          <Badge taskId={taskId} view={v.key} />
        </button>
      ))}
      <div className="spacer" />
      {right}
    </div>
  );
}
