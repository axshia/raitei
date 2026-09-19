/** プロジェクト配下のタスク一覧（担当: WS-F）。クリックでタブを開く */
import { useProjectStore } from "../../store/projectStore";
import { useTabStore } from "../../store/tabStore";

export function TaskList({ projectId }: { projectId: string }) {
  const tasks = useProjectStore((s) => s.tasksByProject[projectId] ?? []);
  const openTask = useTabStore((s) => s.openTask);
  return (
    <ul style={{ listStyle: "none", padding: 0, margin: "6px 0" }}>
      {tasks.map((t) => (
        <li key={t.id} style={{ padding: "3px 6px", cursor: "pointer" }} onClick={() => openTask(t.id)}>
          {t.title} <span className="muted">({t.branch})</span>
        </li>
      ))}
    </ul>
  );
}
