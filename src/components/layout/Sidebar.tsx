/** 左サイドバー（担当: WS-F）: 環境ステータス + プロジェクト一覧 + 各プロジェクトのタスク一覧 */
import { useProjectStore } from "../../store/projectStore";
import { ProjectSection } from "../../features/projects/ProjectSection";
import { AddProjectButtons } from "../../features/projects/AddProjectButtons";

export function Sidebar() {
  const { projects, error, clearError } = useProjectStore();
  return (
    <aside className="sidebar">
      <h3 style={{ margin: "4px 0 12px" }}>raitei</h3>
      <AddProjectButtons />
      {error && (
        <p className="error" onClick={clearError} title="クリックで閉じる">
          {error.message}
        </p>
      )}
      {projects.map((p) => (
        <ProjectSection key={p.id} project={p} />
      ))}
    </aside>
  );
}
