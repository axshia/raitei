/**
 * タスクタブの中身（担当: WS-F）: ヘッダー + サブビュー切替（チャット / PR / コンフリクト / 変更）+ 本体。
 *
 * - 差し込み: チャット = features/chat の ChatPanel（WS-G）、PR = features/pr の PrPanel（WS-H）、
 *   コンフリクト = features/conflicts の ConflictPanel（WS-H）、変更 = features/worktrees の ChangesPanel（WS-F）
 * - 各パネルは taskId だけを受け取り、自分のストアから状態を読む（パネル間の props 依存を作らない）
 * - パネルは `.subview-body`（flex column・高さいっぱい・はみ出したら overflow: auto）の中に置かれ、ルート要素は flex: 1。
 *   余白（padding）はパネル側で付ける。内部スクロールを自前で持つ場合は `.panel-scroll` 等を使う
 * - サブビューは一度表示したらアンマウントせず hidden で保持する（入力中テキスト・スクロール位置の維持）。
 *   非表示中の処理を止めたいパネルは `useIsSubViewVisible(taskId, view)` を使う
 */
import { useState, type ReactNode } from "react";
import { useProjectStore } from "../../store/projectStore";
import { useTabStore, DEFAULT_SUB_VIEW, type TaskSubView } from "../../store/tabStore";
import { ChatPanel } from "../chat/ChatPanel";
import { PrPanel } from "../pr/PrPanel";
import { ConflictPanel } from "../conflicts/ConflictPanel";
import { ChangesPanel } from "../worktrees/ChangesPanel";
import { TaskHeader } from "./TaskHeader";
import { ErrorBoundary } from "../../components/layout/ErrorBoundary";
import { SubViewTabs, SUB_VIEWS } from "../../components/layout/SubViewTabs";

const RENDER: Record<TaskSubView, (taskId: string) => ReactNode> = {
  chat: (id) => <ChatPanel taskId={id} />,
  pr: (id) => <PrPanel taskId={id} />,
  conflicts: (id) => <ConflictPanel taskId={id} />,
  changes: (id) => <ChangesPanel taskId={id} />,
};
const ORDER: TaskSubView[] = ["chat", "pr", "conflicts", "changes"];

export function TaskView({ taskId }: { taskId: string }) {
  const task = useProjectStore((s) => s.findTask(taskId));
  const current = useTabStore((s) => s.subViewByTask[taskId] ?? DEFAULT_SUB_VIEW);
  // 一度表示したサブビューの集合（アンマウントしないため）
  const [visited, setVisited] = useState<Set<TaskSubView>>(() => new Set([current]));
  if (!visited.has(current)) setVisited(new Set(visited).add(current));

  if (!task) {
    return (
      <div className="empty">
        <div>このタスクは見つかりません（削除された可能性があります）</div>
        <button onClick={() => useTabStore.getState().closeTask(taskId)}>タブを閉じる</button>
      </div>
    );
  }

  return (
    <div className="task-view">
      <TaskHeader task={task} />
      <SubViewTabs taskId={taskId} />
      {ORDER.filter((v) => visited.has(v)).map((v) => (
        <div key={v} className={`subview-body subview-${v}`} hidden={v !== current}>
          <ErrorBoundary label={SUB_VIEWS.find((x) => x.key === v)?.label ?? v}>{RENDER[v](taskId)}</ErrorBoundary>
        </div>
      ))}
    </div>
  );
}
