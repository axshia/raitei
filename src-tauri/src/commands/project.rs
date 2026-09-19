//! プロジェクト command（担当: WS-B）。
//!
//! command 関数は薄いラッパーで、処理本体は `AppState` だけに依存する `*_impl` 関数に置く（テスト用）。

use std::path::{Path, PathBuf};

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::git;
use crate::models::{new_id, now, CreateProjectRequest, Project};
use crate::state::{blocking, AppState};

/// 新規作成するリポジトリの既定ブランチ。
const NEW_REPO_DEFAULT_BRANCH: &str = "main";

#[tauri::command]
pub async fn list_projects(state: State<'_, AppState>) -> AppResult<Vec<Project>> {
    state.store.list_projects()
}

/// 既存ローカル git リポジトリを登録する。サブディレクトリが渡されたらトップレベルに正規化。
#[tauri::command]
pub async fn add_project(state: State<'_, AppState>, path: String) -> AppResult<Project> {
    let s = state.inner().clone();
    blocking(move || add_project_impl(&s, &path)).await
}

/// `<parent_dir>/<name>` に新規リポジトリを作成（git init + 初回コミット）して登録する。
#[tauri::command]
pub async fn create_project(state: State<'_, AppState>, req: CreateProjectRequest) -> AppResult<Project> {
    let s = state.inner().clone();
    blocking(move || create_project_impl(&s, &req)).await
}

/// 登録解除のみ（リポジトリ・worktree は削除しない）。配下タスクの実行中エージェントは止める。
#[tauri::command]
pub async fn remove_project(state: State<'_, AppState>, project_id: String) -> AppResult<()> {
    remove_project_impl(&state, &project_id)
}

// ---- 本体 ----

pub(crate) fn add_project_impl(s: &AppState, path: &str) -> AppResult<Project> {
    let input = expand_home(path.trim());
    if input.as_os_str().is_empty() {
        return Err(AppError::InvalidInput("パスを指定してください".into()));
    }
    if !input.is_dir() {
        return Err(AppError::InvalidInput(format!("ディレクトリがありません: {}", input.display())));
    }
    let root = git::repo::repo_root(&s.env, &input)
        .map_err(|_| AppError::InvalidInput(format!("git リポジトリではありません: {}", input.display())))?;
    let root = root.canonicalize()?;
    // linked worktree / submodule は `.git` がファイルになる。タスク用 worktree を誤って登録しないよう弾く。
    if root.join(".git").is_file() {
        return Err(AppError::InvalidInput(format!(
            "worktree またはサブモジュールは登録できません。メインリポジトリを指定してください: {}",
            root.display()
        )));
    }
    let repo_path = root.display().to_string();
    ensure_not_registered(s, &repo_path)?;

    let default_branch = git::repo::default_branch(&s.env, &root)?;
    let p = Project {
        id: new_id(),
        name: dir_name(&root)?,
        repo_path,
        default_branch,
        created_at: now(),
    };
    s.store.insert_project(&p)?;
    Ok(p)
}

pub(crate) fn create_project_impl(s: &AppState, req: &CreateProjectRequest) -> AppResult<Project> {
    let name = req.name.trim();
    validate_repo_name(name)?;
    let parent = expand_home(req.parent_dir.trim());
    if !parent.is_dir() {
        return Err(AppError::InvalidInput(format!("親ディレクトリがありません: {}", parent.display())));
    }
    let path = parent.canonicalize()?.join(name);
    if path.exists() && !is_empty_dir(&path)? {
        return Err(AppError::InvalidInput(format!("既に存在し空ではありません: {}", path.display())));
    }
    let repo_path = path.display().to_string();
    ensure_not_registered(s, &repo_path)?;

    git::repo::init_repo(&s.env, &path, NEW_REPO_DEFAULT_BRANCH)?;
    let p = Project {
        id: new_id(),
        name: name.to_string(),
        repo_path,
        default_branch: NEW_REPO_DEFAULT_BRANCH.into(),
        created_at: now(),
    };
    s.store.insert_project(&p)?;
    Ok(p)
}

pub(crate) fn remove_project_impl(s: &AppState, project_id: &str) -> AppResult<()> {
    s.store.get_project(project_id)?;
    for t in s.store.list_tasks(project_id)? {
        s.agents.cancel(&t.id)?;
    }
    s.store.delete_project(project_id)
}

// ---- 補助 ----

fn ensure_not_registered(s: &AppState, repo_path: &str) -> AppResult<()> {
    if let Some(p) = s.store.list_projects()?.into_iter().find(|p| p.repo_path == repo_path) {
        return Err(AppError::InvalidInput(format!("登録済みです: {} ({})", p.name, p.repo_path)));
    }
    Ok(())
}

fn dir_name(path: &Path) -> AppResult<String> {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| AppError::InvalidInput(format!("ディレクトリ名を取得できません: {}", path.display())))
}

/// 先頭の `~` / `~/` をホームディレクトリに展開する（ダイアログ以外からの手入力向け）。
pub(crate) fn expand_home(p: &str) -> PathBuf {
    if p == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(p));
    }
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(p)
}

/// 新規リポジトリ名（ディレクトリ名）として妥当か。
pub(crate) fn validate_repo_name(name: &str) -> AppResult<()> {
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0'])
        || name.chars().any(char::is_control);
    if bad {
        return Err(AppError::InvalidInput(format!("リポジトリ名が不正です: {name:?}")));
    }
    Ok(())
}

fn is_empty_dir(path: &Path) -> AppResult<bool> {
    Ok(path.is_dir() && std::fs::read_dir(path)?.next().is_none())
}

// ---- テスト ----

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::Path;
    use std::sync::Arc;

    use crate::agent::{AgentEventEnvelope, AgentManager, EventSink};
    use crate::shell_env::ShellEnv;
    use crate::state::AppState;
    use crate::store::Store;

    struct NullSink;
    impl EventSink for NullSink {
        fn emit(&self, _: &AgentEventEnvelope) {}
    }

    pub fn state() -> AppState {
        AppState {
            env: ShellEnv::resolve(),
            store: Arc::new(Store::open_in_memory().unwrap()),
            agents: Arc::new(AgentManager::new()),
            sink: Arc::new(NullSink),
        }
    }

    /// テスト用の git 実行（コミット用の identity を付ける）。
    pub fn git(s: &AppState, cwd: &Path, args: &[&str]) -> String {
        let mut all = vec!["-c", "user.name=raitei-test", "-c", "user.email=test@example.com"];
        all.extend_from_slice(args);
        crate::git::git(&s.env, cwd, &all).unwrap()
    }

    /// `dir` に main ブランチ・初回コミット付きのリポジトリを作る。
    pub fn init_repo(s: &AppState, dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        git(s, dir, &["init", "-q", "-b", "main"]);
        git(s, dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use crate::models::{AgentKind, PermissionLevel, Task};

    #[test]
    fn repo_name_validation() {
        for ok in ["app", "my-app", "app.v2", "日本語"] {
            assert!(validate_repo_name(ok).is_ok(), "{ok}");
        }
        for ng in ["", ".", "..", "a/b", "a\\b", "a\nb"] {
            assert!(validate_repo_name(ng).is_err(), "{ng:?}");
        }
    }

    #[test]
    fn home_expansion() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_home("~"), home);
        assert_eq!(expand_home("~/src/app"), home.join("src/app"));
        assert_eq!(expand_home("/abs/~x"), PathBuf::from("/abs/~x"));
    }

    #[test]
    fn add_project_normalizes_subdir_and_rejects_duplicates() {
        let s = state();
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("myrepo");
        init_repo(&s, &repo);
        std::fs::create_dir_all(repo.join("sub/dir")).unwrap();

        let p = add_project_impl(&s, repo.join("sub/dir").to_str().unwrap()).unwrap();
        assert_eq!(p.name, "myrepo");
        assert_eq!(PathBuf::from(&p.repo_path), repo.canonicalize().unwrap());
        assert_eq!(p.default_branch, "main");
        assert_eq!(s.store.list_projects().unwrap(), vec![p.clone()]);

        // 同じリポジトリ（別の書き方）は重複
        let err = add_project_impl(&s, repo.to_str().unwrap()).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)), "{err:?}");
    }

    #[test]
    fn add_project_rejects_non_repo_and_missing_dir() {
        let s = state();
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(add_project_impl(&s, tmp.path().to_str().unwrap()), Err(AppError::InvalidInput(_))));
        let missing = tmp.path().join("missing");
        assert!(matches!(add_project_impl(&s, missing.to_str().unwrap()), Err(AppError::InvalidInput(_))));
        assert!(matches!(add_project_impl(&s, "  "), Err(AppError::InvalidInput(_))));
        assert!(s.store.list_projects().unwrap().is_empty());
    }

    #[test]
    fn add_project_rejects_linked_worktree() {
        let s = state();
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("r");
        init_repo(&s, &repo);
        let wt = tmp.path().join("wt");
        git(&s, &repo, &["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
        assert!(matches!(add_project_impl(&s, wt.to_str().unwrap()), Err(AppError::InvalidInput(_))));
    }

    #[test]
    fn create_project_validates_before_touching_disk() {
        let s = state();
        let tmp = tempfile::tempdir().unwrap();
        let req = |parent: &Path, name: &str| CreateProjectRequest {
            parent_dir: parent.display().to_string(),
            name: name.into(),
        };
        // 名前不正
        assert!(matches!(create_project_impl(&s, &req(tmp.path(), "a/b")), Err(AppError::InvalidInput(_))));
        // 親ディレクトリなし
        assert!(matches!(
            create_project_impl(&s, &req(&tmp.path().join("nope"), "x")),
            Err(AppError::InvalidInput(_))
        ));
        // 空でない既存ディレクトリ
        std::fs::create_dir_all(tmp.path().join("busy")).unwrap();
        std::fs::write(tmp.path().join("busy/file"), "x").unwrap();
        assert!(matches!(create_project_impl(&s, &req(tmp.path(), "busy")), Err(AppError::InvalidInput(_))));
        assert!(s.store.list_projects().unwrap().is_empty());
    }

    #[test]
    fn create_project_initializes_repo() {
        let s = state();
        let tmp = tempfile::tempdir().unwrap();
        let req = CreateProjectRequest {
            parent_dir: tmp.path().display().to_string(),
            name: "fresh".into(),
        };
        let p = create_project_impl(&s, &req).unwrap();
        assert_eq!(p.name, "fresh");
        assert_eq!(p.default_branch, "main");
        let root = crate::git::repo::repo_root(&s.env, Path::new(&p.repo_path)).unwrap();
        assert_eq!(root.canonicalize().unwrap(), tmp.path().join("fresh").canonicalize().unwrap());
        assert_eq!(s.store.list_projects().unwrap(), vec![p]);
    }

    #[test]
    fn remove_project_deletes_rows_only() {
        let s = state();
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("r");
        init_repo(&s, &repo);
        let p = add_project_impl(&s, repo.to_str().unwrap()).unwrap();
        let ts = now();
        let t = Task {
            id: new_id(),
            project_id: p.id.clone(),
            title: "t".into(),
            branch: "b".into(),
            base_branch: "main".into(),
            worktree_path: "/nowhere".into(),
            agent: AgentKind::Claude,
            permission: PermissionLevel::Safe,
            agent_session_id: None,
            pr_number: None,
            created_at: ts.clone(),
            updated_at: ts,
        };
        s.store.insert_task(&t).unwrap();

        remove_project_impl(&s, &p.id).unwrap();
        assert!(s.store.list_projects().unwrap().is_empty());
        assert!(matches!(s.store.get_task(&t.id), Err(AppError::NotFound(_))));
        assert!(repo.join(".git").is_dir(), "リポジトリ自体は残す");
        assert!(matches!(remove_project_impl(&s, &p.id), Err(AppError::NotFound(_))));
    }
}
