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

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct ShellEnv {
    /// 解決済みの PATH（`:` 区切り）
    pub path: String,
}

impl ShellEnv {
    /// ログインシェルから PATH を解決する。失敗時は現在の PATH + Homebrew 等の既定パスで代替する。
    ///
    /// 仮実装: 現プロセスの PATH に既定パスを足すだけ。WS-A が `$SHELL -ilc` 解決を実装する。
    pub fn resolve() -> Self {
        let current = std::env::var("PATH").unwrap_or_default();
        let home = dirs::home_dir().unwrap_or_default();
        let extras = [
            "/opt/homebrew/bin".to_string(),
            "/usr/local/bin".to_string(),
            home.join(".local/bin").display().to_string(),
            home.join(".cargo/bin").display().to_string(),
        ];
        let mut parts: Vec<String> = current.split(':').filter(|s| !s.is_empty()).map(String::from).collect();
        for e in extras {
            if !parts.contains(&e) {
                parts.push(e);
            }
        }
        ShellEnv { path: parts.join(":") }
    }

    /// PATH 上の実行ファイルを探す。
    pub fn which(&self, cmd: &str) -> Option<PathBuf> {
        self.path
            .split(':')
            .map(|d| Path::new(d).join(cmd))
            .find(|p| p.is_file())
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
        .output()
        .map_err(|e| AppError::Command(format!("{program} の起動に失敗: {e}")))?;
    Ok(CmdOutput {
        status: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}
