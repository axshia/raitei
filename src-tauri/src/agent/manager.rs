//! エージェント実行マネージャ（担当: WS-E）。
//!
//! 責務:
//! - [`AgentRunner`] が組み立てたコマンドを tokio で起動（cwd = worktree, PATH = ShellEnv）
//! - stdout を行単位で読み `parse_line` → [`Store::append_agent_event`] で永続化 + seq 採番 → [`EventSink`] へ配信
//! - stderr は `AgentEvent::Stderr` として配信
//! - `SessionStarted` を受けたら `Store::set_task_agent_session` で session_id を保存（次回 resume 用）
//! - 終了時に必ず `RunFinished` を 1 回配信
//! - タスクごとに同時 1 run。実行中に送信されたら `AppError::Agent` を返す
//! - `cancel` で子プロセスを kill し `RunFinished { cancelled: true }`
//!
//! 契約（シグネチャ凍結）: `new` / `start_run` / `cancel` / `run_state`。
//!
//! 実装メモ:
//! - 子プロセスは独立したプロセスグループで起動し、キャンセル時はグループ全体に SIGTERM を送る
//!   （claude / codex が起動したシェルコマンドも止めるため）。[`KILL_GRACE`] 以内に終わらなければ SIGKILL
//! - 1 run のイベントはすべて 1 つの非同期タスクから順に送るので、seq の順と配信順は一致する
//! - 実行中フラグは RunFinished を送る前に外す（RunFinished を受けたフロントがすぐ次を送れるように）
//! - resume したのにセッションが確立せず異常終了した場合は、会話のリセットを促す Error を追加で送る

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::sync::{mpsc, oneshot};

use crate::error::{AppError, AppResult};
use crate::models::{new_id, AgentKind, Task};
use crate::shell_env::ShellEnv;
use crate::store::Store;

use super::runner::{runner_for, AgentRunner, CommandSpec, ParseState, RunSpec};
use super::types::{AgentEvent, AgentEventEnvelope, AgentRunInfo, AgentRunState};

/// イベント配信先。本番は Tauri の `AppHandle::emit(AGENT_EVENT, ..)`、テストでは Vec に貯める。
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, envelope: &AgentEventEnvelope);
}

/// run 実行に必要な依存。
#[derive(Clone)]
pub struct RunContext {
    pub env: ShellEnv,
    pub store: Arc<Store>,
    pub sink: Arc<dyn EventSink>,
}

/// キャンセル時、SIGTERM を送ってから SIGKILL に切り替えるまでの猶予。
const KILL_GRACE: Duration = Duration::from_secs(3);
/// プロセス終了後、残りの出力を待つ上限（バックグラウンドの孫プロセスがパイプを握り続ける場合に備える）。
const DRAIN_GRACE: Duration = Duration::from_secs(2);
/// 異常終了時のエラーメッセージに含める stderr の末尾行数。
const STDERR_TAIL_LINES: usize = 5;
/// Claude Code のターミナルから raitei を起動した場合（`pnpm tauri dev` など）に継承される、
/// 子の claude を「入れ子セッション」として振る舞わせる環境変数。子プロセスには渡さない。
const NESTED_SESSION_ENV: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
];

/// 実行中 run の管理情報。
struct ActiveRun {
    run_id: String,
    /// キャンセル通知。1 回送ったら None
    cancel: Option<oneshot::Sender<()>>,
}

/// task_id → 実行中 run
type RunningMap = Arc<Mutex<HashMap<String, ActiveRun>>>;

#[derive(Default)]
pub struct AgentManager {
    running: RunningMap,
}

impl AgentManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// タスクの worktree でエージェントに 1 ターン実行させる。即座に返り、イベントは非同期で配信される。
    ///
    /// `UserMessage` を記録してから子プロセスを起動する。worktree や CLI が見つからない場合、
    /// または実行中の場合は何も記録せずに Err を返す。
    pub fn start_run(&self, ctx: &RunContext, task: &Task, prompt: String) -> AppResult<AgentRunInfo> {
        let runner = runner_for(task.agent);
        let spec = RunSpec {
            cwd: PathBuf::from(&task.worktree_path),
            prompt: prompt.clone(),
            resume_session_id: task.agent_session_id.clone().filter(|s| !s.is_empty()),
            permission: task.permission,
            model: None,
        };
        if !spec.cwd.is_dir() {
            return Err(AppError::NotFound(format!("worktree が見つかりません: {}", task.worktree_path)));
        }
        let command = runner.build_command(&spec);
        let program = ctx.env.which(&command.program).ok_or_else(|| {
            AppError::Agent(format!(
                "`{}` が見つかりません。インストール済みか、ログインシェルの PATH に含まれているか確認してください",
                command.program
            ))
        })?;

        let run_id = new_id();
        let (cancel_tx, cancel_rx) = oneshot::channel();
        {
            let mut running = lock(&self.running);
            if running.contains_key(&task.id) {
                return Err(AppError::Agent("このタスクではエージェントが実行中です".into()));
            }
            running.insert(
                task.id.clone(),
                ActiveRun {
                    run_id: run_id.clone(),
                    cancel: Some(cancel_tx),
                },
            );
        }

        let run = Run {
            ctx: ctx.clone(),
            task_id: task.id.clone(),
            run_id: run_id.clone(),
            agent: task.agent,
        };
        if let Err(e) = run.emit(AgentEvent::UserMessage { text: prompt }) {
            release(&self.running, &run.task_id, &run.run_id);
            return Err(e);
        }

        let job = Job {
            runner,
            program,
            command,
            cwd: spec.cwd,
            resume_session_id: spec.resume_session_id,
        };
        let guard = FinishGuard {
            run,
            running: self.running.clone(),
            outcome: None,
        };
        tauri::async_runtime::spawn(async move {
            let outcome = execute(&guard.run, job, cancel_rx).await;
            // guard の drop で実行中フラグを外し、RunFinished を送る
            guard.finish(outcome);
        });

        Ok(AgentRunInfo {
            run_id,
            task_id: task.id.clone(),
            agent: task.agent,
        })
    }

    /// 実行中の run を中断する。実行中でなければ何もしない。
    ///
    /// 停止の完了は `RunFinished { cancelled: true }` で通知される（この関数は待たない）。
    pub fn cancel(&self, task_id: &str) -> AppResult<()> {
        let sender = lock(&self.running).get_mut(task_id).and_then(|r| r.cancel.take());
        if let Some(tx) = sender {
            let _ = tx.send(());
        }
        Ok(())
    }

    pub fn run_state(&self, task_id: &str) -> AgentRunState {
        let run_id = lock(&self.running).get(task_id).map(|r| r.run_id.clone());
        AgentRunState {
            task_id: task_id.to_string(),
            running: run_id.is_some(),
            run_id,
        }
    }
}

fn lock(running: &RunningMap) -> MutexGuard<'_, HashMap<String, ActiveRun>> {
    running.lock().unwrap_or_else(|e| e.into_inner())
}

/// 自分の run が登録されたままなら外す。
fn release(running: &RunningMap, task_id: &str, run_id: &str) {
    let mut map = lock(running);
    if map.get(task_id).is_some_and(|r| r.run_id == run_id) {
        map.remove(task_id);
    }
}

/// 1 run のイベント送信先。
struct Run {
    ctx: RunContext,
    task_id: String,
    run_id: String,
    agent: AgentKind,
}

impl Run {
    fn emit(&self, event: AgentEvent) -> AppResult<()> {
        let envelope = self
            .ctx
            .store
            .append_agent_event(&self.task_id, &self.run_id, self.agent, event)?;
        self.ctx.sink.emit(&envelope);
        Ok(())
    }

    /// 送信失敗（タスク削除済みなど）は run を止めずにログだけ残す。
    fn emit_logged(&self, event: AgentEvent) {
        if let Err(e) = self.emit(event) {
            eprintln!("[raitei] エージェントイベントの保存に失敗しました (task {}): {e}", self.task_id);
        }
    }
}

/// run の終了処理を必ず 1 回行うためのガード（非同期タスクが途中で落ちても RunFinished を送る）。
struct FinishGuard {
    run: Run,
    running: RunningMap,
    outcome: Option<Outcome>,
}

impl FinishGuard {
    fn finish(mut self, outcome: Outcome) {
        self.outcome = Some(outcome);
    }
}

impl Drop for FinishGuard {
    fn drop(&mut self) {
        let outcome = self.outcome.take().unwrap_or(Outcome {
            exit_code: None,
            cancelled: false,
        });
        release(&self.running, &self.run.task_id, &self.run.run_id);
        self.run.emit_logged(AgentEvent::RunFinished {
            exit_code: outcome.exit_code,
            cancelled: outcome.cancelled,
        });
    }
}

struct Job {
    runner: Box<dyn AgentRunner>,
    /// 解決済みの実行ファイルパス
    program: PathBuf,
    command: CommandSpec,
    cwd: PathBuf,
    resume_session_id: Option<String>,
}

struct Outcome {
    exit_code: Option<i32>,
    cancelled: bool,
}

enum Line {
    Stdout(String),
    Stderr(String),
}

/// 子プロセスを起動し、終了するまで出力をイベントに変換して送る。
async fn execute(run: &Run, job: Job, cancel_rx: oneshot::Receiver<()>) -> Outcome {
    let Job {
        runner,
        program,
        command,
        cwd,
        resume_session_id,
    } = job;
    let name = command.program.clone();

    let mut cmd = run.ctx.env.tokio_command(&program.to_string_lossy());
    cmd.args(&command.args)
        .current_dir(&cwd)
        .stdin(if command.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for key in NESTED_SESSION_ENV {
        cmd.env_remove(key);
    }
    #[cfg(unix)]
    cmd.process_group(0);

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            run.emit_logged(AgentEvent::Error {
                message: format!("{name} の起動に失敗しました: {e}"),
            });
            return Outcome {
                exit_code: None,
                cancelled: false,
            };
        }
    };
    let pid = child.id();

    if let (Some(input), Some(mut stdin)) = (command.stdin, child.stdin.take()) {
        tauri::async_runtime::spawn(async move {
            // 子が stdin を読まずに終了した場合の書き込みエラーは無視する。drop で EOF になる
            let _ = stdin.write_all(input.as_bytes()).await;
        });
    }

    let (tx, mut rx) = mpsc::unbounded_channel();
    let readers = [
        child
            .stdout
            .take()
            .map(|out| tauri::async_runtime::spawn(read_lines(out, tx.clone(), Line::Stdout))),
        child
            .stderr
            .take()
            .map(|err| tauri::async_runtime::spawn(read_lines(err, tx.clone(), Line::Stderr))),
    ];
    drop(tx);

    let mut parse = ParseState::default();
    let mut saved_session = resume_session_id.clone();
    let mut session_started = false;
    let mut saw_result = false;
    let mut stderr_tail: VecDeque<String> = VecDeque::new();

    let mut cancel_rx = Some(cancel_rx);
    let mut kill_timer: Option<oneshot::Receiver<()>> = None;
    let mut drain_timer: Option<oneshot::Receiver<()>> = None;
    let mut status: Option<std::io::Result<ExitStatus>> = None;
    let mut cancelled = false;
    let mut lines_open = true;

    while status.is_none() || lines_open {
        tokio::select! {
            requested = wait_signal(&mut cancel_rx) => {
                cancel_rx = None;
                if requested && status.is_none() {
                    cancelled = true;
                    signal_process_group(&run.ctx.env, pid, "TERM", &mut child);
                    kill_timer = Some(timer(KILL_GRACE));
                }
            }
            _ = wait_signal(&mut kill_timer) => {
                kill_timer = None;
                if status.is_none() {
                    signal_process_group(&run.ctx.env, pid, "KILL", &mut child);
                }
            }
            exited = child.wait(), if status.is_none() => {
                status = Some(exited);
                drain_timer = Some(timer(DRAIN_GRACE));
            }
            line = rx.recv(), if lines_open => match line {
                Some(Line::Stdout(line)) => {
                    for event in runner.parse_line(&line, &mut parse) {
                        match &event {
                            AgentEvent::SessionStarted { session_id, .. } => {
                                session_started = true;
                                if saved_session.as_deref() != Some(session_id.as_str()) {
                                    match run.ctx.store.set_task_agent_session(&run.task_id, Some(session_id)) {
                                        Ok(()) => saved_session = Some(session_id.clone()),
                                        Err(e) => run.emit_logged(AgentEvent::Error {
                                            message: format!("セッション ID の保存に失敗しました: {e}"),
                                        }),
                                    }
                                }
                            }
                            AgentEvent::Result { .. } => saw_result = true,
                            _ => {}
                        }
                        run.emit_logged(event);
                    }
                }
                Some(Line::Stderr(line)) => {
                    if !line.trim().is_empty() {
                        if stderr_tail.len() == STDERR_TAIL_LINES {
                            stderr_tail.pop_front();
                        }
                        stderr_tail.push_back(line.clone());
                        run.emit_logged(AgentEvent::Stderr { line });
                    }
                }
                None => lines_open = false,
            },
            _ = wait_signal(&mut drain_timer) => {
                // プロセスは終了したがパイプが閉じない（孫プロセスが保持している）
                break;
            }
        }
    }
    for reader in readers.into_iter().flatten() {
        reader.abort();
    }

    let exit_code = match &status {
        Some(Ok(s)) => s.code(),
        _ => None,
    };
    let success = matches!(&status, Some(Ok(s)) if s.success());
    if !cancelled && !success {
        if !saw_result {
            let mut message = match &status {
                Some(Ok(s)) => match s.code() {
                    Some(code) => format!("{name} が終了コード {code} で終了しました"),
                    None => format!("{name} がシグナルで終了しました"),
                },
                Some(Err(e)) => format!("{name} の終了を確認できませんでした: {e}"),
                None => format!("{name} の終了を確認できませんでした"),
            };
            if !stderr_tail.is_empty() {
                message.push('\n');
                message.push_str(&Vec::from(stderr_tail).join("\n"));
            }
            run.emit_logged(AgentEvent::Error { message });
        }
        if let (Some(id), false) = (&resume_session_id, session_started) {
            run.emit_logged(AgentEvent::Error {
                message: format!(
                    "前回の会話（セッション {id}）を再開できませんでした。解決しない場合は会話をリセットしてから送り直してください。"
                ),
            });
        }
    }
    Outcome { exit_code, cancelled }
}

/// パイプを行単位で読み、チャネルに流す。不正な UTF-8 は置換文字にする。
async fn read_lines<R: AsyncRead + Unpin>(reader: R, tx: mpsc::UnboundedSender<Line>, wrap: fn(String) -> Line) {
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string();
                if tx.send(wrap(line)).is_err() {
                    break;
                }
            }
        }
    }
}

/// `Some(rx)` なら通知を待って `true`（送信側が落ちたら `false`）。`None` なら永久に待つ。
async fn wait_signal(rx: &mut Option<oneshot::Receiver<()>>) -> bool {
    match rx {
        Some(rx) => rx.await.is_ok(),
        None => std::future::pending().await,
    }
}

/// `duration` 後に通知するタイマー（tokio の time 機能を使わずに済ませるためスレッドで待つ）。
fn timer(duration: Duration) -> oneshot::Receiver<()> {
    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        std::thread::sleep(duration);
        let _ = tx.send(());
    });
    rx
}

/// 子プロセスのグループ全体にシグナルを送る。失敗したら子プロセス本体だけを kill する。
fn signal_process_group(env: &ShellEnv, pid: Option<u32>, signal: &str, child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = pid {
        let sent = env
            .command("kill")
            .args([format!("-{signal}"), "--".to_string(), format!("-{pid}")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if sent {
            return;
        }
    }
    #[cfg(not(unix))]
    let _ = (env, pid, signal);
    let _ = child.start_kill();
}

#[cfg(test)]
mod tests {
    //! 偽の `claude` / `codex` スクリプトを PATH に置いて、プロセス管理の一連の流れを確かめる。
    //! 実機の CLI を使う確認は `tests::real_*`（`#[ignore]`）で手動実行する。

    use super::*;
    use crate::models::{now, PermissionLevel, Project};
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::time::Instant;

    #[derive(Default)]
    struct VecSink(Mutex<Vec<AgentEventEnvelope>>);

    impl EventSink for VecSink {
        fn emit(&self, envelope: &AgentEventEnvelope) {
            self.0.lock().unwrap().push(envelope.clone());
        }
    }

    impl VecSink {
        fn events(&self) -> Vec<AgentEventEnvelope> {
            self.0.lock().unwrap().clone()
        }

        fn run_events(&self, run_id: &str) -> Vec<AgentEvent> {
            self.events()
                .into_iter()
                .filter(|e| e.run_id == run_id)
                .map(|e| e.event)
                .collect()
        }

        /// 条件を満たすまで待つ（最大 `secs` 秒）。
        fn wait_until(&self, secs: u64, mut pred: impl FnMut(&[AgentEventEnvelope]) -> bool) -> bool {
            let deadline = Instant::now() + Duration::from_secs(secs);
            while Instant::now() < deadline {
                if pred(&self.events()) {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            false
        }

        fn wait_finished(&self, run_id: &str, secs: u64) {
            let done = self.wait_until(secs, |evs| {
                evs.iter()
                    .any(|e| e.run_id == run_id && matches!(e.event, AgentEvent::RunFinished { .. }))
            });
            assert!(done, "RunFinished が来ない: {:#?}", self.events());
        }
    }

    struct Harness {
        _dir: tempfile::TempDir,
        bin: PathBuf,
        work: PathBuf,
        ctx: RunContext,
        sink: Arc<VecSink>,
        manager: AgentManager,
    }

    impl Harness {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let bin = dir.path().join("bin");
            let work = dir.path().join("work");
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::create_dir_all(&work).unwrap();
            let sink = Arc::new(VecSink::default());
            let ctx = RunContext {
                // kill（プロセスグループへのシグナル送信）のために /bin も含める
                env: ShellEnv {
                    path: format!("{}:/bin:/usr/bin", bin.display()),
                },
                store: Arc::new(Store::open_in_memory().unwrap()),
                sink: sink.clone(),
            };
            Harness {
                _dir: dir,
                bin,
                work,
                ctx,
                sink,
                manager: AgentManager::new(),
            }
        }

        /// PATH 上に偽の CLI を置く。`$WORK` は作業ディレクトリに置換する。
        fn fake_cli(&self, name: &str, body: &str) {
            let path = self.bin.join(name);
            let script = format!("#!/bin/sh\n{}\n", body.replace("$WORK", &self.work.display().to_string()));
            std::fs::write(&path, script).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        fn task(&self, agent: AgentKind, session: Option<&str>) -> Task {
            let project = Project {
                id: new_id(),
                name: "p".into(),
                repo_path: self.work.display().to_string(),
                default_branch: "main".into(),
                created_at: now(),
            };
            self.ctx.store.insert_project(&project).unwrap();
            let task = Task {
                id: new_id(),
                project_id: project.id,
                title: "t".into(),
                branch: "b".into(),
                base_branch: "main".into(),
                worktree_path: self.work.display().to_string(),
                agent,
                permission: PermissionLevel::Safe,
                agent_session_id: session.map(String::from),
                pr_number: None,
                created_at: now(),
                updated_at: now(),
            };
            self.ctx.store.insert_task(&task).unwrap();
            task
        }

        fn start(&self, task_id: &str, prompt: &str) -> AppResult<AgentRunInfo> {
            let task = self.ctx.store.get_task(task_id).unwrap();
            self.manager.start_run(&self.ctx, &task, prompt.into())
        }

        fn read(&self, file: &str) -> String {
            std::fs::read_to_string(self.work.join(file)).unwrap_or_default()
        }
    }

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    /// プロセスが残っているか（回収待ちのゾンビを考慮して最大 2 秒待つ）。
    fn process_alive(pid: &str) -> bool {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let alive = std::process::Command::new("/bin/kill")
                .args(["-0", pid.trim()])
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if !alive || Instant::now() >= deadline {
                return alive;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn run_streams_events_saves_session_and_resumes() {
        let h = Harness::new();
        h.fake_cli(
            "claude",
            &format!(
                "echo \"$@\" > \"$WORK/args.txt\"\n/bin/cat > \"$WORK/stdin.txt\"\n/bin/cat '{}'\necho 'warn: noisy' >&2",
                fixture("claude_stream.jsonl").display()
            ),
        );
        let task = h.task(AgentKind::Claude, None);

        let info = h.start(&task.id, "こんにちは").unwrap();
        assert_eq!(info.agent, AgentKind::Claude);
        h.sink.wait_finished(&info.run_id, 10);

        let events = h.sink.run_events(&info.run_id);
        assert_eq!(events.first(), Some(&AgentEvent::UserMessage { text: "こんにちは".into() }));
        assert_eq!(
            events.last(),
            Some(&AgentEvent::RunFinished {
                exit_code: Some(0),
                cancelled: false,
            })
        );
        let kinds: Vec<_> = events
            .iter()
            .filter(|e| !matches!(e, AgentEvent::Stderr { .. }))
            .map(|e| serde_json::to_value(e).unwrap()["type"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            kinds,
            ["user_message", "session_started", "tool_use", "tool_result", "assistant_text", "result", "run_finished"]
        );
        assert!(events.contains(&AgentEvent::Stderr { line: "warn: noisy".into() }));

        // seq は 1 からの連番で、配信順と一致し、ストアの履歴とも一致する
        let delivered = h.sink.events();
        let seqs: Vec<u64> = delivered.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, (1..=delivered.len() as u64).collect::<Vec<_>>());
        assert_eq!(h.ctx.store.list_agent_events(&task.id, None).unwrap(), delivered);

        // プロンプトは stdin で渡り、新規なので --resume は付かない
        assert_eq!(h.read("stdin.txt"), "こんにちは");
        assert!(!h.read("args.txt").contains("--resume"));

        // session_id が保存され、実行中フラグも外れている
        let sid = "67c70e97-50d2-481f-a02f-9b7e03fc15ac";
        assert_eq!(h.ctx.store.get_task(&task.id).unwrap().agent_session_id.as_deref(), Some(sid));
        assert!(!h.manager.run_state(&task.id).running);

        // 2 回目は保存した session_id で resume する
        let info2 = h.start(&task.id, "続けて").unwrap();
        h.sink.wait_finished(&info2.run_id, 10);
        assert!(h.read("args.txt").contains(&format!("--resume {sid}")), "{}", h.read("args.txt"));
        assert_eq!(h.read("stdin.txt"), "続けて");
    }

    #[test]
    fn rejects_concurrent_run_and_cancel_kills_process_group() {
        let h = Harness::new();
        // セッション開始行を出したあと、孫プロセス（sleep）を待ち続ける
        h.fake_cli(
            "codex",
            "/bin/cat > /dev/null\necho '{\"type\":\"thread.started\",\"thread_id\":\"th-1\"}'\n/bin/sleep 30 &\necho $! > \"$WORK/child.pid\"\nwait",
        );
        let task = h.task(AgentKind::Codex, None);

        let info = h.start(&task.id, "long job").unwrap();
        assert!(h.sink.wait_until(10, |evs| evs
            .iter()
            .any(|e| matches!(e.event, AgentEvent::SessionStarted { .. }))));
        let state = h.manager.run_state(&task.id);
        assert!(state.running);
        assert_eq!(state.run_id.as_deref(), Some(info.run_id.as_str()));

        // 実行中の送信は拒否し、何も記録しない
        let before = h.sink.events().len();
        assert!(matches!(h.start(&task.id, "again"), Err(AppError::Agent(_))));
        assert_eq!(h.sink.events().len(), before);

        let started = Instant::now();
        h.manager.cancel(&task.id).unwrap();
        h.sink.wait_finished(&info.run_id, 10);
        assert!(started.elapsed() < KILL_GRACE, "SIGTERM で止まるはず: {:?}", started.elapsed());

        let events = h.sink.run_events(&info.run_id);
        assert_eq!(
            events.last(),
            Some(&AgentEvent::RunFinished {
                exit_code: None,
                cancelled: true,
            })
        );
        // キャンセルは異常終了扱いにしない
        assert!(!events.iter().any(|e| matches!(e, AgentEvent::Error { .. })), "{events:#?}");
        assert!(!h.manager.run_state(&task.id).running);
        // グループ全体に届くので孫プロセスも止まっている
        let pid = h.read("child.pid");
        assert!(!pid.trim().is_empty());
        assert!(!process_alive(&pid), "sleep (pid {pid}) が残っている");
        // キャンセル前に確立したセッションは保存済み（次回 resume できる）
        assert_eq!(h.ctx.store.get_task(&task.id).unwrap().agent_session_id.as_deref(), Some("th-1"));

        // 実行中でなければ cancel は何もしない
        h.manager.cancel(&task.id).unwrap();
        h.manager.cancel("unknown-task").unwrap();
    }

    #[test]
    fn failed_resume_reports_exit_code_stderr_and_hint() {
        let h = Harness::new();
        // 存在しない thread_id を resume したときの codex の実際の振る舞い（JSONL なし・stderr・終了コード 1）
        h.fake_cli(
            "codex",
            "echo \"$@\" > \"$WORK/args.txt\"\n/bin/cat > /dev/null\necho 'Error: thread/resume: thread/resume failed: no rollout found for thread id old-thread (code -32600)' >&2\nexit 1",
        );
        let task = h.task(AgentKind::Codex, Some("old-thread"));

        let info = h.start(&task.id, "続き").unwrap();
        h.sink.wait_finished(&info.run_id, 10);
        assert!(h.read("args.txt").starts_with("exec resume --json"), "{}", h.read("args.txt"));

        let events = h.sink.run_events(&info.run_id);
        let errors: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Error { message } => Some(message.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(errors.len(), 2, "{events:#?}");
        assert!(errors[0].contains("終了コード 1"), "{}", errors[0]);
        assert!(errors[0].contains("no rollout found"), "{}", errors[0]);
        assert!(errors[1].contains("old-thread"), "{}", errors[1]);
        assert_eq!(
            events.last(),
            Some(&AgentEvent::RunFinished {
                exit_code: Some(1),
                cancelled: false,
            })
        );
        // 自動では消さない（ユーザーがリセットするまで保持）
        assert_eq!(
            h.ctx.store.get_task(&task.id).unwrap().agent_session_id.as_deref(),
            Some("old-thread")
        );
    }

    #[test]
    fn result_error_does_not_duplicate_exit_error() {
        let h = Harness::new();
        h.fake_cli(
            "codex",
            &format!(
                "/bin/cat > /dev/null\n/bin/cat '{}'\nexit 1",
                fixture("codex_turn_failed.jsonl").display()
            ),
        );
        let task = h.task(AgentKind::Codex, None);
        let info = h.start(&task.id, "x").unwrap();
        h.sink.wait_finished(&info.run_id, 10);
        let events = h.sink.run_events(&info.run_id);
        assert!(events.iter().any(|e| matches!(e, AgentEvent::Result { is_error: true, .. })));
        // Result が出ていれば終了コードのエラーは重ねない（codex 自身の警告・エラーの 3 件のみ）
        let errors = events.iter().filter(|e| matches!(e, AgentEvent::Error { .. })).count();
        assert_eq!(errors, 3, "{events:#?}");
    }

    #[test]
    fn missing_cli_or_worktree_is_rejected_without_events() {
        let h = Harness::new();
        let task = h.task(AgentKind::Claude, None);
        // bin に claude が無い
        assert!(matches!(h.start(&task.id, "x"), Err(AppError::Agent(_))));

        let mut gone = task.clone();
        gone.worktree_path = h.work.join("missing").display().to_string();
        assert!(matches!(
            h.manager.start_run(&h.ctx, &gone, "x".into()),
            Err(AppError::NotFound(_))
        ));

        assert!(h.sink.events().is_empty());
        assert!(!h.manager.run_state(&task.id).running);
        assert!(Path::new(&task.worktree_path).is_dir());
    }

    #[test]
    fn background_grandchild_holding_stdout_does_not_hang() {
        let h = Harness::new();
        // 本体は即終了するが、stdout を握った孫プロセスが残る
        h.fake_cli(
            "claude",
            "/bin/cat > /dev/null\necho '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"s-1\"}'\n/bin/sleep 20 &\necho $! > \"$WORK/child.pid\"\nexit 0",
        );
        let task = h.task(AgentKind::Claude, None);
        let info = h.start(&task.id, "x").unwrap();
        h.sink.wait_finished(&info.run_id, 10);
        assert_eq!(
            h.sink.run_events(&info.run_id).last(),
            Some(&AgentEvent::RunFinished {
                exit_code: Some(0),
                cancelled: false,
            })
        );
        let _ = std::process::Command::new("/bin/kill").arg(h.read("child.pid").trim()).status();
    }

    // ---- 実機の CLI を使う確認（手動実行: `cargo test --lib real_ -- --ignored --nocapture`） ----

    /// 実機 CLI で「ツール実行中にキャンセル → 同じセッションを resume して会話が続く」ことを確かめる。
    fn real_cancel_then_resume(agent: AgentKind, permission: PermissionLevel) {
        let dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(VecSink::default());
        let ctx = RunContext {
            env: ShellEnv::resolve(),
            store: Arc::new(Store::open_in_memory().unwrap()),
            sink: sink.clone(),
        };
        let h = Harness {
            bin: dir.path().to_path_buf(),
            work: dir.path().to_path_buf(),
            _dir: dir,
            ctx,
            sink,
            manager: AgentManager::new(),
        };
        let mut task = h.task(agent, None);
        task.permission = permission;
        h.ctx.store.update_task(&task).unwrap();

        let first = h
            .start(
                &task.id,
                "Remember the codeword PINEAPPLE. Then run the shell command `sleep 30` and after it finishes reply DONE.",
            )
            .unwrap();
        let saw_tool = h.sink.wait_until(180, |evs| {
            evs.iter()
                .any(|e| e.run_id == first.run_id && matches!(e.event, AgentEvent::ToolUse { .. }))
        });
        assert!(saw_tool, "ToolUse が来ない: {:#?}", h.sink.events());
        let started = Instant::now();
        h.manager.cancel(&task.id).unwrap();
        h.sink.wait_finished(&first.run_id, 10);
        println!("cancel → RunFinished: {:?}", started.elapsed());
        assert!(matches!(
            h.sink.run_events(&first.run_id).last(),
            Some(AgentEvent::RunFinished { cancelled: true, .. })
        ));
        let sid = h.ctx.store.get_task(&task.id).unwrap().agent_session_id;
        assert!(sid.is_some(), "session_id が保存されていない");

        let second = h
            .start(&task.id, "What codeword did I ask you to remember? Reply with only the codeword.")
            .unwrap();
        h.sink.wait_finished(&second.run_id, 180);
        let events = h.sink.run_events(&second.run_id);
        for e in &events {
            println!("{}", serde_json::to_string(e).unwrap().chars().take(200).collect::<String>());
        }
        assert!(
            events
                .iter()
                .any(|e| matches!(e, AgentEvent::SessionStarted { session_id, .. } if Some(session_id) == sid.as_ref())),
            "resume したのに別のセッションになった"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, AgentEvent::AssistantText { text } if text.to_uppercase().contains("PINEAPPLE"))),
            "resume 後に会話が続いていない"
        );
        assert!(matches!(
            events.last(),
            Some(AgentEvent::RunFinished {
                exit_code: Some(0),
                cancelled: false,
            })
        ));
    }

    #[test]
    #[ignore = "実機の claude CLI を呼ぶ（課金あり）"]
    fn real_claude_cancel_then_resume() {
        real_cancel_then_resume(AgentKind::Claude, PermissionLevel::Full);
    }

    #[test]
    #[ignore = "実機の codex CLI を呼ぶ（課金あり）"]
    fn real_codex_cancel_then_resume() {
        real_cancel_then_resume(AgentKind::Codex, PermissionLevel::Safe);
    }
}
