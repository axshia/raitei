//! ログインシェルの PATH 解決と外部コマンド実行ヘルパー（担当: WS-A）。
//!
//! GUI（Finder / Dock）から起動した macOS アプリは `/usr/bin:/bin:...` 程度の PATH しか持たないため、
//! `$SHELL -ilc` でログインシェルの PATH を 1 回だけ取得してキャッシュし、全ての外部コマンド
//! （git / gh / claude / codex）の起動に使う。
//!
//! 契約（シグネチャ凍結）:
//! - [`ShellEnv::resolve`] 起動時に 1 回呼ぶ
//! - [`ShellEnv::which`] コマンドの絶対パスを返す
//! - [`ShellEnv::command`] PATH 設定済みの `std::process::Command` を返す
//! - [`run`] 同期実行して [`CmdOutput`] を返す
//!
//! 追加の公開 API（契約外・追加のみ）:
//! - [`run_with_timeout`] タイムアウト付きの同期実行（`--version` 取得など、ハングし得る呼び出し用）

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};

/// ログインシェルの出力から PATH を切り出すためのマーカー。
const MARKER: &str = "__RAITEI__";
/// ログインシェル起動のタイムアウト。
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);
/// `$SHELL` が未設定のときに使うシェル。
const DEFAULT_SHELL: &str = "/bin/zsh";

#[derive(Debug, Clone)]
pub struct ShellEnv {
    /// 解決済みの PATH（`:` 区切り）
    pub path: String,
}

impl ShellEnv {
    /// ログインシェルから PATH を解決する。失敗時は現在の PATH + Homebrew 等の既定パスで代替する。
    ///
    /// ログインシェルで取れた場合も、既定パス（Homebrew など）のうち欠けているものは末尾に補う。
    pub fn resolve() -> Self {
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SHELL.to_string());
        let base = login_shell_path(&shell, RESOLVE_TIMEOUT)
            .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());
        ShellEnv {
            path: merge_paths(&base, &fallback_dirs()),
        }
    }

    /// PATH を明示して作る（テストや将来の設定 UI 用）。
    pub fn from_path(path: impl Into<String>) -> Self {
        ShellEnv { path: path.into() }
    }

    /// PATH 上の実行ファイルを探す。
    pub fn which(&self, cmd: &str) -> Option<PathBuf> {
        if cmd.is_empty() {
            return None;
        }
        if cmd.contains('/') {
            let p = PathBuf::from(cmd);
            return is_executable(&p).then_some(p);
        }
        self.path
            .split(':')
            .filter(|d| !d.is_empty())
            .map(|d| Path::new(d).join(cmd))
            .find(|p| is_executable(p))
    }

    /// PATH を設定済みの Command を作る。`program` は名前でも絶対パスでもよい。
    pub fn command(&self, program: &str) -> Command {
        let resolved = if program.contains('/') {
            PathBuf::from(program)
        } else {
            self.which(program).unwrap_or_else(|| PathBuf::from(program))
        };
        let mut c = Command::new(resolved);
        c.env("PATH", &self.path);
        c
    }

    /// tokio 版（エージェントの長時間プロセス用）。
    pub fn tokio_command(&self, program: &str) -> tokio::process::Command {
        tokio::process::Command::from(self.command(program))
    }
}

/// Homebrew など、GUI 起動時に欠けがちな既定の探索ディレクトリ。
fn fallback_dirs() -> Vec<String> {
    let mut dirs = vec![
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        "/usr/local/bin".to_string(),
        "/usr/bin".to_string(),
        "/bin".to_string(),
        "/usr/sbin".to_string(),
        "/sbin".to_string(),
    ];
    if let Some(home) = dirs::home_dir() {
        for rel in [".local/bin", ".cargo/bin", ".bun/bin", ".volta/bin"] {
            dirs.push(home.join(rel).display().to_string());
        }
    }
    dirs
}

/// `primary` の順序を保ったまま重複と空要素を除き、`extras` のうち欠けているものを末尾に足す。
pub(crate) fn merge_paths(primary: &str, extras: &[String]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for p in primary.split(':').map(str::to_string).chain(extras.iter().cloned()) {
        if !p.is_empty() && !parts.contains(&p) {
            parts.push(p);
        }
    }
    parts.join(":")
}

/// ログインシェルの出力からマーカーで囲まれた PATH を取り出す。
/// rc ファイルの出力が前後に混入しても、最後のマーカー対を採用する。
pub(crate) fn extract_marked_path(output: &str) -> Option<String> {
    let end = output.rfind(MARKER)?;
    let start = output[..end].rfind(MARKER)? + MARKER.len();
    let path = output[start..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// `shell -ilc` で PATH を取得する。失敗・タイムアウト時は None。
pub(crate) fn login_shell_path(shell: &str, timeout: Duration) -> Option<String> {
    let script = format!(r#"printf "{MARKER}%s{MARKER}" "$PATH""#);
    let child = Command::new(shell)
        .args(["-ilc", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // oh-my-zsh の自動更新確認などで対話待ちにならないようにする
        .env("DISABLE_AUTO_UPDATE", "true")
        .spawn()
        .ok()?;
    let out = wait_with_timeout(child, timeout).ok()?;
    extract_marked_path(&out.stdout)
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

#[derive(Debug, Clone)]
pub struct CmdOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CmdOutput {
    pub fn success(&self) -> bool {
        self.status == 0
    }
}

/// 外部コマンドを `cwd` で同期実行する。起動自体に失敗した場合のみ Err。
/// 終了コードが非 0 でも Ok を返すので、呼び出し側で `success()` を確認すること。
pub fn run(env: &ShellEnv, program: &str, args: &[&str], cwd: &Path) -> AppResult<CmdOutput> {
    let out = env
        .command(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| AppError::Command(format!("{program} の起動に失敗: {e}")))?;
    Ok(CmdOutput {
        status: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// [`run`] のタイムアウト付き版。時間内に終わらなければ子プロセスを kill して `AppError::Command` を返す。
pub fn run_with_timeout(
    env: &ShellEnv,
    program: &str,
    args: &[&str],
    cwd: &Path,
    timeout: Duration,
) -> AppResult<CmdOutput> {
    let child = env
        .command(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Command(format!("{program} の起動に失敗: {e}")))?;
    wait_with_timeout(child, timeout)
        .map_err(|e| AppError::Command(format!("{program}: {e}")))
}

/// 子プロセスの終了と stdout / stderr の読み切りを `timeout` まで待つ。
/// 読み取りは別スレッドで行う（パイプ詰まりによるデッドロックを避けるため）。
fn wait_with_timeout(mut child: Child, timeout: Duration) -> Result<CmdOutput, String> {
    let deadline = Instant::now() + timeout;
    let stdout_rx = spawn_reader(child.stdout.take());
    let stderr_rx = spawn_reader(child.stderr.take());

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{} 秒以内に終了しませんでした", timeout.as_secs_f32()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(e) => return Err(format!("終了待ちに失敗: {e}")),
        }
    };

    // 孫プロセスがパイプを握り続ける場合に備え、読み取りも締め切りまでしか待たない
    let recv = |rx: mpsc::Receiver<String>| {
        let remaining = deadline.saturating_duration_since(Instant::now());
        rx.recv_timeout(remaining.max(Duration::from_millis(100)))
            .unwrap_or_default()
    };
    Ok(CmdOutput {
        status: status.code().unwrap_or(-1),
        stdout: recv(stdout_rx),
        stderr: recv(stderr_rx),
    })
}

fn spawn_reader<R: Read + Send + 'static>(src: Option<R>) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut s) = src {
            let _ = s.read_to_end(&mut buf);
        }
        let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[test]
    fn extract_marked_path_ignores_rc_noise() {
        let out = "Welcome!\nsome motd\n__RAITEI__/opt/homebrew/bin:/usr/bin__RAITEI__\n";
        assert_eq!(extract_marked_path(out).as_deref(), Some("/opt/homebrew/bin:/usr/bin"));
    }

    #[test]
    fn extract_marked_path_uses_last_pair() {
        let out = "__RAITEI__old__RAITEI__ noise __RAITEI__/new/bin__RAITEI__";
        assert_eq!(extract_marked_path(out).as_deref(), Some("/new/bin"));
    }

    #[test]
    fn extract_marked_path_rejects_missing_or_empty() {
        assert_eq!(extract_marked_path("no marker"), None);
        assert_eq!(extract_marked_path("__RAITEI__/only/one"), None);
        assert_eq!(extract_marked_path("__RAITEI____RAITEI__"), None);
    }

    #[test]
    fn merge_paths_dedups_and_appends_missing() {
        let merged = merge_paths(
            "/a::/b:/a",
            &["/b".to_string(), "/c".to_string(), String::new()],
        );
        assert_eq!(merged, "/a:/b:/c");
    }

    #[test]
    fn login_shell_path_reads_marker_from_fake_shell() {
        let dir = tempfile::tempdir().unwrap();
        // 引数（-ilc と script）を無視し、rc の雑音 + マーカー付き PATH を出す偽シェル
        let shell = write_script(
            dir.path(),
            "fakesh",
            "echo 'rc noise'; printf '__RAITEI__/x/bin:/y/bin__RAITEI__'",
        );
        let path = login_shell_path(shell.to_str().unwrap(), Duration::from_secs(5));
        assert_eq!(path.as_deref(), Some("/x/bin:/y/bin"));
    }

    #[test]
    fn login_shell_path_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let shell = write_script(dir.path(), "slowsh", "sleep 10");
        let started = Instant::now();
        let path = login_shell_path(shell.to_str().unwrap(), Duration::from_millis(300));
        assert_eq!(path, None);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn login_shell_path_fails_for_missing_shell() {
        assert_eq!(login_shell_path("/nonexistent/shell", Duration::from_secs(1)), None);
    }

    #[test]
    fn real_login_shell_resolves_path() {
        // 実際の /bin/sh（-i -l -c を受け付ける）で PATH が取れること
        let path = login_shell_path("/bin/sh", Duration::from_secs(5));
        assert!(path.is_some_and(|p| p.contains("/usr/bin") || p.contains("/bin")));
    }

    #[test]
    fn which_finds_only_executables() {
        let dir = tempfile::tempdir().unwrap();
        write_script(dir.path(), "mytool", "exit 0");
        fs::write(dir.path().join("notexec"), "x").unwrap();
        let env = ShellEnv::from_path(format!("/nonexistent::{}", dir.path().display()));
        assert_eq!(env.which("mytool"), Some(dir.path().join("mytool")));
        assert_eq!(env.which("notexec"), None);
        assert_eq!(env.which("missing"), None);
        assert_eq!(env.which(""), None);
        let abs = dir.path().join("mytool");
        assert_eq!(env.which(abs.to_str().unwrap()), Some(abs));
    }

    #[test]
    fn command_uses_resolved_path_env() {
        let dir = tempfile::tempdir().unwrap();
        write_script(dir.path(), "printpath", "printf '%s' \"$PATH\"");
        let path = format!("{}:/usr/bin:/bin", dir.path().display());
        let env = ShellEnv::from_path(path.clone());
        let out = run(&env, "printpath", &[], dir.path()).unwrap();
        assert!(out.success());
        assert_eq!(out.stdout, path);
    }

    #[test]
    fn run_reports_nonzero_status_and_spawn_failure() {
        let dir = tempfile::tempdir().unwrap();
        write_script(dir.path(), "fail", "echo err >&2; exit 3");
        let env = ShellEnv::from_path(dir.path().display().to_string());
        let out = run(&env, "fail", &[], dir.path()).unwrap();
        assert_eq!(out.status, 3);
        assert_eq!(out.stderr.trim(), "err");
        assert!(run(&env, "does-not-exist", &[], dir.path()).is_err());
    }

    #[test]
    fn run_with_timeout_kills_slow_command() {
        let dir = tempfile::tempdir().unwrap();
        write_script(dir.path(), "slow", "sleep 10");
        write_script(dir.path(), "fast", "echo hi");
        let env = ShellEnv::from_path(dir.path().display().to_string());
        let started = Instant::now();
        let err = run_with_timeout(&env, "slow", &[], dir.path(), Duration::from_millis(300));
        assert!(err.is_err());
        assert!(started.elapsed() < Duration::from_secs(3));
        let ok = run_with_timeout(&env, "fast", &[], dir.path(), Duration::from_secs(5)).unwrap();
        assert_eq!(ok.stdout.trim(), "hi");
    }

    #[test]
    fn resolve_includes_fallback_dirs() {
        let env = ShellEnv::resolve();
        assert!(env.path.split(':').any(|d| d == "/opt/homebrew/bin"));
        assert!(env.path.split(':').any(|d| d == "/usr/bin"));
    }
}
