/** プロジェクト配下のタスク一覧（担当: WS-F）。クリックでタブを開く。実行中・PR 番号を表示 */
import type { Task } from "../../api";
import { useProjectStore } from "../../store/projectStore";
import { useTabStore } from "../../store/tabStore";
import { useAgentStore } from "../../store/agentStore";

function TaskRow({ task }: { task: Task }) {
  const openTask = useTabStore((s) => s.openTask);
  const active = useTabStore((s) => s.activeTaskId === task.id);
  const isOpen = useTabStore((s) => s.openTaskIds.includes(task.id));
  const running = useAgentStore((s) => !!s.runningByTask[task.id]);

  return (
    <li
      className={`task-row ${active ? "active" : ""} ${isOpen ? "open" : ""}`}
      title={`${task.title}\n${task.branch} ← ${task.baseBranch}\n${task.worktreePath}`}
      onClick={() => openTask(task.id)}
    >
      <span className={`dot ${running ? "running" : isOpen ? "" : "hidden"}`} />
      <span className="task-row-main">
        <span className="task-row-title truncate">{task.title}</span>
        <span className="task-row-branch mono subtle truncate">{task.branch}</span>
      </span>
      {task.prNumber != null && (
        <span
          className="badge"
          title="PR を開く"
          onClick={(e) => {
            e.stopPropagation();
            openTask(task.id, "pr");
          }}
        >
          #{task.prNumber}
        </span>
      )}
      <span className={`agent-mark ${task.agent}`} title={task.agent}>
        {task.agent === "claude" ? "CL" : "CX"}
      </span>
    </li>
  );
}

export function TaskList({ projectId, onNewTask }: { projectId: string; onNewTask?(): void }) {
  const tasks = useProjectStore((s) => s.tasksByProject[projectId]);

  if (!tasks || tasks.length === 0) {
    return (
      <div className="task-list-empty subtle">
        タスクなし
        {onNewTask && (
          <button className="ghost sm" onClick={onNewTask}>
            + 新しいタスク
          </button>
        )}
      </div>
    );
  }
  return (
    <ul className="task-list">
      {tasks.map((t) => (
        <TaskRow key={t.id} task={t} />
      ))}
    </ul>
  );
}
