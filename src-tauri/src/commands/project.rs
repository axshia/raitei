//! プロジェクト command（担当: WS-B）。

use std::path::PathBuf;

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::git;
use crate::models::{new_id, now, CreateProjectRequest, Project};
use crate::state::{blocking, AppState};

#[tauri::command]
pub async fn list_projects(state: State<'_, AppState>) -> AppResult<Vec<Project>> {
    state.store.list_projects()
}

/// 既存ローカル git リポジトリを登録する。サブディレクトリが渡されたらトップレベルに正規化。
#[tauri::command]
pub async fn add_project(state: State<'_, AppState>, path: String) -> AppResult<Project> {
    let s = state.inner().clone();
    blocking(move || {
        let root = git::repo::repo_root(&s.env, &PathBuf::from(&path))?;
        let default_branch = git::repo::default_branch(&s.env, &root)?;
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| AppError::InvalidInput(path.clone()))?;
        let p = Project {
            id: new_id(),
            name,
            repo_path: root.display().to_string(),
            default_branch,
            created_at: now(),
        };
        s.store.insert_project(&p)?;
        Ok(p)
    })
    .await
}

/// `<parent_dir>/<name>` に新規リポジトリを作成（git init + 初回コミット）して登録する。
#[tauri::command]
pub async fn create_project(state: State<'_, AppState>, req: CreateProjectRequest) -> AppResult<Project> {
    let s = state.inner().clone();
    blocking(move || {
        let path = PathBuf::from(&req.parent_dir).join(&req.name);
        git::repo::init_repo(&s.env, &path, "main")?;
        let p = Project {
            id: new_id(),
            name: req.name.clone(),
            repo_path: path.display().to_string(),
            default_branch: "main".into(),
            created_at: now(),
        };
        s.store.insert_project(&p)?;
        Ok(p)
    })
    .await
}

/// 登録解除のみ（リポジトリ・worktree は削除しない）。
#[tauri::command]
pub async fn remove_project(state: State<'_, AppState>, project_id: String) -> AppResult<()> {
    state.store.delete_project(&project_id)
}
