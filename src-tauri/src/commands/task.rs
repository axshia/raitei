//! タスク command（担当: WS-B）。
//!
//! タスク作成 = 入力検証 → worktree パス決定 → `git worktree add` → DB 登録（失敗時は worktree を戻す）。
//! タスク削除 = 実行中エージェント停止 → (任意) worktree 削除 / ブランチ削除 → DB 削除。
//!
//! command 関数は薄いラッパーで、処理本体は `AppState` だけに依存する `*_impl` 関数に置く（テスト用）。

use std::path::PathBuf;

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::git;
use crate::models::{new_id, now, CreateTaskRequest, DeleteTaskOptions, Task, UpdateTaskRequest};
use crate::state::{blocking, AppState};

#[tauri::command]
pub async fn list_tasks(state: State<'_, AppState>, project_id: String) -> AppResult<Vec<Task>> {
    state.store.list_tasks(&project_id)
}

#[tauri::command]
pub async fn get_task(state: State<'_, AppState>, task_id: String) -> AppResult<Task> {
    state.store.get_task(&task_id)
}

#[tauri::command]
pub async fn create_task(state: State<'_, AppState>, req: CreateTaskRequest) -> AppResult<Task> {
    let s = state.inner().clone();
    blocking(move || create_task_impl(&s, req)).await
}

/// タイトル・エージェント種別・権限の変更。エージェント種別を変えた場合は session をリセットする。
#[tauri::command]
pub async fn update_task(state: State<'_, AppState>, req: UpdateTaskRequest) -> AppResult<Task> {
    update_task_impl(&state, req)
}

#[tauri::command]
pub async fn delete_task(state: State<'_, AppState>, task_id: String, options: DeleteTaskOptions) -> AppResult<()> {
    let s = state.inner().clone();
    blocking(move || delete_task_impl(&s, &task_id, &options)).await
}

// ---- 本体 ----

pub(crate) fn create_task_impl(s: &AppState, req: CreateTaskRequest) -> AppResult<Task> {
    let branch = req.branch.trim().to_string();
    if !git::repo::is_valid_branch_name(&branch) {
        return Err(AppError::InvalidInput(format!("ブランチ名が不正です: {branch:?}")));
    }
    let project = s.store.get_project(&req.project_id)?;
    let base = req
        .base_branch
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .unwrap_or(&project.default_branch)
        .to_string();
    if !git::repo::is_valid_branch_name(&base) {
        return Err(AppError::InvalidInput(format!("base ブランチ名が不正です: {base:?}")));
    }
    if base == branch {
        return Err(AppError::InvalidInput(format!("base と同じブランチでは作れません: {branch}")));
    }
    if s.store.list_tasks(&project.id)?.iter().any(|t| t.branch == branch) {
        return Err(AppError::InvalidInput(format!("ブランチ {branch} のタスクは既にあります")));
    }
    let repo = PathBuf::from(&project.repo_path);
    let wt = git::worktree::worktree_path_for(&repo, &branch);
    if wt.exists() {
        return Err(AppError::InvalidInput(format!("worktree の配置先が既に存在します: {}", wt.display())));
    }

    git::worktree::add_worktree(&s.env, &repo, &wt, &branch, &base)?;

    let title = match req.title.trim() {
        "" => branch.clone(),
        t => t.to_string(),
    };
    let ts = now();
    let t = Task {
        id: new_id(),
        project_id: project.id,
        title,
        branch,
        base_branch: base,
        worktree_path: wt.display().to_string(),
        agent: req.agent,
        permission: req.permission,
        agent_session_id: None,
        pr_number: None,
        created_at: ts.clone(),
        updated_at: ts,
    };
    if let Err(e) = s.store.insert_task(&t) {
        // DB 登録に失敗したら worktree を残さない（ブランチは既存だった可能性があるので消さない）。
        let _ = git::worktree::remove_worktree(&s.env, &repo, &wt, true);
        return Err(e);
    }
    Ok(t)
}

pub(crate) fn update_task_impl(s: &AppState, req: UpdateTaskRequest) -> AppResult<Task> {
    let mut t = s.store.get_task(&req.task_id)?;
    if let Some(title) = req.title {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::InvalidInput("タイトルを入力してください".into()));
        }
        t.title = title.to_string();
    }
    if let Some(agent) = req.agent {
        if agent != t.agent {
            if s.agents.run_state(&t.id).running {
                return Err(AppError::Agent("実行中はエージェントを切り替えられません".into()));
            }
            t.agent = agent;
            // セッション ID はエージェント固有なので引き継げない。
            t.agent_session_id = None;
        }
    }
    if let Some(p) = req.permission {
        t.permission = p;
    }
    t.updated_at = now();
    s.store.update_task(&t)?;
    Ok(t)
}

pub(crate) fn delete_task_impl(s: &AppState, task_id: &str, options: &DeleteTaskOptions) -> AppResult<()> {
    let t = s.store.get_task(task_id)?;
    if options.delete_branch && !options.remove_worktree {
        // worktree でチェックアウト中のブランチは git が削除を拒否する。
        return Err(AppError::InvalidInput("ブランチを削除するには worktree も削除してください".into()));
    }
    let project = s.store.get_project(&t.project_id)?;
    let repo = PathBuf::from(&project.repo_path);
    s.agents.cancel(&t.id)?;
    if options.remove_worktree {
        let wt = PathBuf::from(&t.worktree_path);
        if wt.exists() {
            git::worktree::remove_worktree(&s.env, &repo, &wt, options.force)?;
        } else if repo.is_dir() {
            // ディレクトリが手動で消されている場合は git の管理情報だけ掃除する。
            git::git(&s.env, &repo, &["worktree", "prune"])?;
        }
    }
    if options.delete_branch {
        git::worktree::delete_branch(&s.env, &repo, &t.branch, options.force)?;
    }
    s.store.delete_task(&t.id)
}

// ---- テスト ----

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::commands::project::add_project_impl;
    use crate::commands::project::test_support::{git as test_git, init_repo, state};
    use crate::models::{AgentKind, PermissionLevel, Project};

    fn setup() -> (AppState, tempfile::TempDir, Project) {
        let s = state();
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("app");
        init_repo(&s, &repo);
        let p = add_project_impl(&s, repo.to_str().unwrap()).unwrap();
        (s, tmp, p)
    }

    fn req(p: &Project, branch: &str) -> CreateTaskRequest {
        CreateTaskRequest {
            project_id: p.id.clone(),
            title: "  ".into(),
            branch: branch.into(),
            base_branch: None,
            agent: AgentKind::Claude,
            permission: PermissionLevel::Safe,
        }
    }

    /// worktree を作らずに DB へ直接タスクを入れる（WS-C 非依存のテスト用）。
    fn insert_raw(s: &AppState, p: &Project, branch: &str, worktree: &Path) -> Task {
        let ts = now();
        let t = Task {
            id: new_id(),
            project_id: p.id.clone(),
            title: branch.into(),
            branch: branch.into(),
            base_branch: "main".into(),
            worktree_path: worktree.display().to_string(),
            agent: AgentKind::Claude,
            permission: PermissionLevel::Safe,
            agent_session_id: Some("sess".into()),
            pr_number: None,
            created_at: ts.clone(),
            updated_at: ts,
        };
        s.store.insert_task(&t).unwrap();
        t
    }

    #[test]
    fn create_task_validation() {
        let (s, _tmp, p) = setup();
        for bad in ["", "-x", "a..b", "has space", "trail/", "x.lock"] {
            assert!(matches!(create_task_impl(&s, req(&p, bad)), Err(AppError::InvalidInput(_))), "{bad:?}");
        }
        // base と同じ
        assert!(matches!(create_task_impl(&s, req(&p, "main")), Err(AppError::InvalidInput(_))));
        // プロジェクトなし
        let mut r = req(&p, "feat/x");
        r.project_id = "missing".into();
        assert!(matches!(create_task_impl(&s, r), Err(AppError::NotFound(_))));
        // 同じブランチのタスクが既にある
        insert_raw(&s, &p, "feat/dup", Path::new("/nowhere"));
        assert!(matches!(create_task_impl(&s, req(&p, "feat/dup")), Err(AppError::InvalidInput(_))));
        // worktree 配置先が既に存在
        let wt = git::worktree::worktree_path_for(Path::new(&p.repo_path), "feat/occupied");
        std::fs::create_dir_all(&wt).unwrap();
        assert!(matches!(create_task_impl(&s, req(&p, "feat/occupied")), Err(AppError::InvalidInput(_))));
        assert_eq!(s.store.list_tasks(&p.id).unwrap().len(), 1);
    }

    #[test]
    fn create_and_delete_task_with_worktree() {
        let (s, _tmp, p) = setup();
        let t = match create_task_impl(&s, req(&p, " feat/login ")) {
            // WS-C の add_worktree が未実装の間はスキップ（結合後に有効になる）
            Err(AppError::NotImplemented(_)) => return,
            r => r.unwrap(),
        };
        assert_eq!(t.branch, "feat/login");
        assert_eq!(t.title, "feat/login", "空タイトルはブランチ名で補う");
        assert_eq!(t.base_branch, "main");
        let expected = git::worktree::worktree_path_for(Path::new(&p.repo_path), "feat/login");
        assert_eq!(PathBuf::from(&t.worktree_path), expected);
        assert!(expected.is_dir());
        let head = test_git(&s, &expected, &["branch", "--show-current"]);
        assert_eq!(head.trim(), "feat/login");
        assert_eq!(s.store.list_tasks(&p.id).unwrap(), vec![t.clone()]);

        match delete_task_impl(
            &s,
            &t.id,
            &DeleteTaskOptions {
                remove_worktree: true,
                delete_branch: true,
                force: true,
            },
        ) {
            Err(AppError::NotImplemented(_)) => return,
            r => r.unwrap(),
        }
        assert!(!expected.exists());
        let branches = test_git(&s, Path::new(&p.repo_path), &["branch", "--list", "feat/login"]);
        assert!(branches.trim().is_empty());
        assert!(s.store.list_tasks(&p.id).unwrap().is_empty());
    }

    #[test]
    fn update_task_fields_and_session_reset() {
        let (s, _tmp, p) = setup();
        let t = insert_raw(&s, &p, "b", Path::new("/nowhere"));

        // 同じエージェントならセッションは維持
        let u = update_task_impl(
            &s,
            UpdateTaskRequest {
                task_id: t.id.clone(),
                title: Some("  新しい題  ".into()),
                agent: Some(AgentKind::Claude),
                permission: Some(PermissionLevel::Full),
            },
        )
        .unwrap();
        assert_eq!(u.title, "新しい題");
        assert_eq!(u.permission, PermissionLevel::Full);
        assert_eq!(u.agent_session_id.as_deref(), Some("sess"));

        // エージェントを変えるとセッションをリセット
        let u = update_task_impl(
            &s,
            UpdateTaskRequest {
                task_id: t.id.clone(),
                title: None,
                agent: Some(AgentKind::Codex),
                permission: None,
            },
        )
        .unwrap();
        assert_eq!(u.agent, AgentKind::Codex);
        assert_eq!(u.agent_session_id, None);
        assert_eq!(u.title, "新しい題");
        assert_eq!(s.store.get_task(&t.id).unwrap(), u);

        // 空タイトル・存在しないタスク
        let empty = UpdateTaskRequest {
            task_id: t.id.clone(),
            title: Some(" ".into()),
            agent: None,
            permission: None,
        };
        assert!(matches!(update_task_impl(&s, empty), Err(AppError::InvalidInput(_))));
        let missing = UpdateTaskRequest {
            task_id: "nope".into(),
            title: None,
            agent: None,
            permission: None,
        };
        assert!(matches!(update_task_impl(&s, missing), Err(AppError::NotFound(_))));
    }

    #[test]
    fn delete_task_db_only_and_option_validation() {
        let (s, tmp, p) = setup();
        let wt = tmp.path().join("keep-me");
        std::fs::create_dir_all(&wt).unwrap();
        let t = insert_raw(&s, &p, "b", &wt);
        s.store
            .append_agent_event(&t.id, "r", AgentKind::Claude, crate::agent::AgentEvent::Stderr { line: "x".into() })
            .unwrap();

        // ブランチだけ削除は不可
        let bad = DeleteTaskOptions {
            remove_worktree: false,
            delete_branch: true,
            force: false,
        };
        assert!(matches!(delete_task_impl(&s, &t.id, &bad), Err(AppError::InvalidInput(_))));

        // DB のみ削除（ファイルは残す）
        delete_task_impl(&s, &t.id, &DeleteTaskOptions::default()).unwrap();
        assert!(wt.is_dir());
        assert!(matches!(s.store.get_task(&t.id), Err(AppError::NotFound(_))));
        assert!(s.store.list_agent_events(&t.id, None).unwrap().is_empty());
        assert!(matches!(
            delete_task_impl(&s, &t.id, &DeleteTaskOptions::default()),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn delete_task_with_missing_worktree_dir_prunes() {
        let (s, tmp, p) = setup();
        // git worktree として登録後、ディレクトリだけ手動で消えた状態を作る
        let wt = tmp.path().join("gone");
        test_git(&s, Path::new(&p.repo_path), &["worktree", "add", "-q", "-b", "gone", wt.to_str().unwrap()]);
        std::fs::remove_dir_all(&wt).unwrap();
        let t = insert_raw(&s, &p, "gone", &wt);

        let opts = DeleteTaskOptions {
            remove_worktree: true,
            delete_branch: false,
            force: false,
        };
        delete_task_impl(&s, &t.id, &opts).unwrap();
        let list = test_git(&s, Path::new(&p.repo_path), &["worktree", "list", "--porcelain"]);
        assert!(!list.contains("gone"), "prune されていること: {list}");
        assert!(s.store.list_tasks(&p.id).unwrap().is_empty());
    }
}
