/** タスクタブバー（担当: WS-F）。実行中インジケータは agentStore.runningByTask を参照する */
import { useProjectStore } from "../../store/projectStore";
import { useTabStore } from "../../store/tabStore";
import { useAgentStore } from "../../store/agentStore";

export function TaskTabs() {
  const { openTaskIds, activeTaskId, activateTask, closeTask } = useTabStore();
  const findTask = useProjectStore((s) => s.findTask);
  const running = useAgentStore((s) => s.runningByTask);

  if (openTaskIds.length === 0) return null;
  return (
    <nav className="tabs">
      {openTaskIds.map((id) => {
        const t = findTask(id);
        return (
          <div key={id} className={`tab ${id === activeTaskId ? "active" : ""}`} onClick={() => activateTask(id)}>
            {running[id] ? "● " : ""}
            {t?.title ?? id}
            <span
              style={{ marginLeft: 8 }}
              onClick={(e) => {
                e.stopPropagation();
                closeTask(id);
              }}
            >
              ×
            </span>
          </div>
        );
      })}
    </nav>
  );
}
