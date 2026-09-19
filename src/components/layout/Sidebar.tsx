/** 左サイドバー（担当: WS-F）: 環境ステータス + プロジェクト追加 + プロジェクト一覧（各プロジェクトのタスク一覧） */
import { useProjectStore } from "../../store/projectStore";
import { ProjectSection } from "../../features/projects/ProjectSection";
import { AddProjectButtons } from "../../features/projects/AddProjectButtons";
import { EnvStatus } from "./EnvStatus";

export function Sidebar() {
  const projects = useProjectStore((s) => s.projects);
  const loaded = useProjectStore((s) => s.loaded);
  const error = useProjectStore((s) => s.error);
  const clearError = useProjectStore((s) => s.clearError);

  return (
    <aside className="sidebar">
      <header className="sidebar-header" data-tauri-drag-region="deep">
        <span className="brand">raitei</span>
        <AddProjectButtons />
      </header>

      <div className="sidebar-body">
        <div className="sidebar-caption section-title">
          <span>プロジェクト</span>
          <span className="subtle">{projects.length || ""}</span>
        </div>
        {!loaded && (
          <div className="sidebar-note muted">
            <span className="spinner" /> 読み込み中…
          </div>
        )}
        {loaded && projects.length === 0 && (
          <div className="sidebar-note muted">
            まだプロジェクトがありません。右上の「追加」から既存の git リポジトリを登録するか、新規作成してください。
          </div>
        )}
        {projects.map((p) => (
          <ProjectSection key={p.id} project={p} />
        ))}
      </div>

      {error && (
        <div className="banner danger sidebar-error">
          <span className="grow selectable">
            <b>{error.kind}</b> {error.message}
          </span>
          <button className="ghost icon sm" onClick={clearError} aria-label="閉じる">
            ×
          </button>
        </div>
      )}
      <EnvStatus />
    </aside>
  );
}
