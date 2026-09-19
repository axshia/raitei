/** サイドバー内の 1 プロジェクト（担当: WS-F）: 名前 + タスク一覧 + 新規タスク */
import type { Project } from "../../api";
import { TaskList } from "../tasks/TaskList";
import { NewTaskForm } from "../tasks/NewTaskForm";

export function ProjectSection({ project }: { project: Project }) {
  return (
    <section style={{ marginBottom: 16 }}>
      <div title={project.repoPath} style={{ fontWeight: 600 }}>
        {project.name}
      </div>
      <div className="muted" style={{ fontSize: 11 }}>
        {project.repoPath}
      </div>
      <TaskList projectId={project.id} />
      <NewTaskForm project={project} />
    </section>
  );
}
