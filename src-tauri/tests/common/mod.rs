//! IPC 経由の結合テストの共通部品。
//!
//! Tauri の `MockRuntime` に本番と同じ command 一覧（`raitei_lib::invoke_handler`）を登録し、
//! フロントの `src/api/*.ts` と同じ command 名・camelCase 引数で呼び出す。
//! git は一時ディレクトリ内のラッパー経由で実行し、ユーザーのグローバル設定（署名・フック）に左右されないようにする。

#![allow(dead_code)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{WebviewWindow, WebviewWindowBuilder};

use raitei_lib::agent::{AgentEventEnvelope, AgentManager, EventSink};
use raitei_lib::shell_env::ShellEnv;
use raitei_lib::state::AppState;
use raitei_lib::store::Store;

/// 配信されたエージェントイベントを貯める。
#[derive(Default)]
pub struct VecSink(Mutex<Vec<AgentEventEnvelope>>);

impl EventSink for VecSink {
    fn emit(&self, envelope: &AgentEventEnvelope) {
        self.0.lock().unwrap().push(envelope.clone());
    }
}

impl VecSink {
    pub fn events(&self) -> Vec<AgentEventEnvelope> {
        self.0.lock().unwrap().clone()
    }

    /// run の `run_finished` が届くまで待つ。
    pub fn wait_finished(&self, run_id: &str, timeout: Duration) -> Vec<AgentEventEnvelope> {
        let deadline = Instant::now() + timeout;
        loop {
            let evs: Vec<_> = self.events().into_iter().filter(|e| e.run_id == run_id).collect();
            if evs.iter().any(|e| e.event_type() == "run_finished") {
                return evs;
            }
            assert!(Instant::now() < deadline, "run {run_id} が終わらない: {evs:#?}");
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

pub trait EventType {
    fn event_type(&self) -> String;
}

impl EventType for AgentEventEnvelope {
    fn event_type(&self) -> String {
        serde_json::to_value(&self.event).unwrap()["type"].as_str().unwrap().to_string()
    }
}

pub struct Harness {
    pub dir: tempfile::TempDir,
    /// PATH の先頭に置くディレクトリ（git ラッパー・偽の gh など）
    pub bin: PathBuf,
    pub sink: Arc<VecSink>,
    pub state: AppState,
    webview: WebviewWindow<MockRuntime>,
    _app: tauri::App<MockRuntime>,
}

impl Harness {
    /// 一時ディレクトリに git ラッパーを置き、その後ろに現在の PATH を続ける。
    pub fn new() -> Self {
        Self::with_base_path(std::env::var("PATH").unwrap())
    }

    /// ログインシェルの PATH（claude / codex / gh の実機確認用）の前に git ラッパーを置く。
    pub fn with_login_shell_path() -> Self {
        Self::with_base_path(ShellEnv::resolve().path)
    }

    fn with_base_path(base: String) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let real_git = ShellEnv::from_path(base.clone()).which("git").expect("git が見つからない");
        write_script(
            &bin.join("git"),
            &format!(
                "#!/bin/sh\nunset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_CONFIG_COUNT\n\
                 export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_TERMINAL_PROMPT=0\n\
                 exec {} -c user.name=raitei-test -c user.email=test@example.invalid -c commit.gpgsign=false \
                 -c core.hooksPath=/dev/null -c init.defaultBranch=main \"$@\"\n",
                sh_quote(&real_git.display().to_string())
            ),
        );
        let env = ShellEnv::from_path(format!("{}:{}", bin.display(), base));
        let sink = Arc::new(VecSink::default());
        let state = AppState {
            env,
            store: Arc::new(Store::open(&dir.path().join("raitei.db")).unwrap()),
            agents: Arc::new(AgentManager::new()),
            sink: sink.clone(),
        };
        let app = mock_builder()
            .invoke_handler(raitei_lib::invoke_handler())
            .manage(state.clone())
            .build(mock_context(noop_assets()))
            .unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
        Harness {
            dir,
            bin,
            sink,
            state,
            webview,
            _app: app,
        }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// フロントの `invoke(cmd, args)` と同じ形で呼ぶ。Err はフロントに渡る `{ kind, message }`。
    pub fn invoke(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        let request = InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        get_ipc_response(&self.webview, request).map(|b| b.deserialize::<Value>().unwrap())
    }

    pub fn ok(&self, cmd: &str, args: Value) -> Value {
        self.invoke(cmd, args.clone())
            .unwrap_or_else(|e| panic!("{cmd} が失敗: {e}\nargs: {args}"))
    }

    /// 失敗を期待する。`kind` を検証して message を返す。
    pub fn err(&self, cmd: &str, args: Value, kind: &str) -> String {
        match self.invoke(cmd, args.clone()) {
            Ok(v) => panic!("{cmd} が成功してしまった: {v}\nargs: {args}"),
            Err(e) => {
                assert_eq!(e["kind"], kind, "{cmd} のエラー種別: {e}");
                e["message"].as_str().unwrap().to_string()
            }
        }
    }

    /// テスト側から git を実行する（ラッパー経由）。
    pub fn git(&self, cwd: &Path, args: &[&str]) -> String {
        raitei_lib::git::git(&self.state.env, cwd, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    }

    /// main ブランチと初回コミットを持つリポジトリを作る。
    pub fn init_repo(&self, path: &Path, files: &[(&str, &str)]) {
        std::fs::create_dir_all(path).unwrap();
        self.git(path, &["init", "-q", "-b", "main"]);
        for (name, body) in files {
            write(path, name, body);
        }
        self.git(path, &["add", "-A"]);
        self.git(path, &["commit", "-q", "--allow-empty", "-m", "init"]);
    }

    pub fn commit_all(&self, cwd: &Path, message: &str) {
        self.git(cwd, &["add", "-A"]);
        self.git(cwd, &["commit", "-q", "-m", message]);
    }

    /// 一時ディレクトリに bare リポジトリを作って origin に設定し、main を push する。
    pub fn add_local_origin(&self, repo: &Path) -> PathBuf {
        let remote = self.root().join("origin.git");
        self.git(self.root(), &["init", "-q", "--bare", "-b", "main", remote.to_str().unwrap()]);
        self.git(repo, &["remote", "add", "origin", remote.to_str().unwrap()]);
        self.git(repo, &["push", "-q", "-u", "origin", "main"]);
        remote
    }
}

pub fn write(dir: &Path, name: &str, body: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

pub fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap()
}

pub fn write_script(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub fn s(v: &Value) -> &str {
    v.as_str().unwrap_or_else(|| panic!("文字列ではない: {v}"))
}
