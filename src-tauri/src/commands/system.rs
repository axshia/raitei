//! 環境情報 command（担当: WS-A）。

use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use tauri::State;

use crate::error::AppResult;
use crate::github;
use crate::models::{EnvironmentInfo, ToolInfo};
use crate::shell_env::{run_with_timeout, ShellEnv};
use crate::state::{blocking, AppState};

/// `--version` 1 回あたりのタイムアウト（claude は起動が遅いことがあるため長めにする）。
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// 各 CLI の検出結果（絶対パス・バージョン）と gh 認証状態。
/// 外部コマンドは並列に実行し、待ち時間を最も遅い 1 本分に抑える。
#[tauri::command]
pub async fn get_environment(state: State<'_, AppState>) -> AppResult<EnvironmentInfo> {
    let env = state.env.clone();
    blocking(move || Ok(collect_environment(&env))).await
}

/// 環境情報を集める（Tauri に依存しない本体）。
pub(crate) fn collect_environment(env: &ShellEnv) -> EnvironmentInfo {
    thread::scope(|s| {
        let git = s.spawn(|| detect_tool(env, "git"));
        let gh = s.spawn(|| detect_tool(env, "gh"));
        let claude = s.spawn(|| detect_tool(env, "claude"));
        let codex = s.spawn(|| detect_tool(env, "codex"));
        let gh_auth = s.spawn(|| env.which("gh").is_some() && github::gh::is_authenticated(env));
        let join = |h: thread::ScopedJoinHandle<'_, ToolInfo>, name: &str| {
            h.join().unwrap_or_else(|_| ToolInfo {
                name: name.to_string(),
                path: None,
                version: None,
            })
        };
        EnvironmentInfo {
            path: env.path.clone(),
            git: join(git, "git"),
            gh: join(gh, "gh"),
            claude: join(claude, "claude"),
            codex: join(codex, "codex"),
            gh_authenticated: gh_auth.join().unwrap_or(false),
        }
    })
}

/// 1 つの CLI の絶対パスとバージョンを調べる。見つからなければ path / version とも None。
pub(crate) fn detect_tool(env: &ShellEnv, name: &str) -> ToolInfo {
    let path = env.which(name);
    let version = path.as_deref().and_then(|p| tool_version(env, p));
    ToolInfo {
        name: name.to_string(),
        path: path.map(|p| p.display().to_string()),
        version,
    }
}

fn tool_version(env: &ShellEnv, program: &Path) -> Option<String> {
    let cwd = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let out = run_with_timeout(
        env,
        &program.display().to_string(),
        &["--version"],
        &cwd,
        VERSION_TIMEOUT,
    )
    .ok()?;
    if !out.success() {
        return None;
    }
    parse_version(&out.stdout).or_else(|| parse_version(&out.stderr))
}

/// `--version` の出力からバージョン番号を取り出す。
///
/// 例: `git version 2.50.1 (Apple Git-155)` → `2.50.1`、`2.1.277 (Claude Code)` → `2.1.277`、
/// `codex-cli 0.155.0` → `0.155.0`、`gh version 2.83.0 (2025-11-04)` → `2.83.0`。
/// 数字始まりでドットを含むトークンが無い場合は、最初の空でない行をそのまま返す。
pub(crate) fn parse_version(output: &str) -> Option<String> {
    let first_line = output.lines().map(str::trim).find(|l| !l.is_empty())?;
    let token = output
        .split_whitespace()
        .map(|t| t.trim_start_matches(['v', 'V']).trim_matches(|c: char| matches!(c, '(' | ')' | ',')))
        .find(|t| {
            t.starts_with(|c: char| c.is_ascii_digit())
                && t.contains('.')
                && t.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
                // 日付（2025-11-04 など）は除外する
                && !t.chars().filter(|c| *c == '-').nth(1).is_some()
        });
    Some(token.map(str::to_string).unwrap_or_else(|| first_line.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn parse_version_variants() {
        let cases = [
            ("git version 2.50.1 (Apple Git-155)\n", "2.50.1"),
            ("2.1.277 (Claude Code)\n", "2.1.277"),
            ("codex-cli 0.155.0\n", "0.155.0"),
            (
                "gh version 2.83.0 (2025-11-04)\nhttps://github.com/cli/cli/releases/tag/v2.83.0\n",
                "2.83.0",
            ),
            ("tool v1.2.3-beta.1\n", "1.2.3-beta.1"),
        ];
        for (input, want) in cases {
            assert_eq!(parse_version(input).as_deref(), Some(want), "input: {input:?}");
        }
    }

    #[test]
    fn parse_version_falls_back_to_first_line() {
        assert_eq!(parse_version("\n  weird tool build\n").as_deref(), Some("weird tool build"));
        assert_eq!(parse_version("   \n"), None);
    }

    #[test]
    fn detect_tool_reports_path_and_version() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("fakecli");
        fs::write(&p, "#!/bin/sh\necho 'fakecli version 9.8.7'\n").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        let env = ShellEnv::from_path(format!("{}:/usr/bin:/bin", dir.path().display()));

        let info = detect_tool(&env, "fakecli");
        assert_eq!(info.path.as_deref(), Some(p.to_str().unwrap()));
        assert_eq!(info.version.as_deref(), Some("9.8.7"));

        let missing = detect_tool(&env, "no-such-cli");
        assert_eq!(missing.name, "no-such-cli");
        assert!(missing.path.is_none() && missing.version.is_none());
    }

    #[test]
    fn collect_environment_without_tools() {
        let dir = tempfile::tempdir().unwrap();
        let env = ShellEnv::from_path(dir.path().display().to_string());
        let info = collect_environment(&env);
        assert_eq!(info.path, env.path);
        assert!(info.claude.path.is_none());
        assert!(info.gh.path.is_none());
        assert!(!info.gh_authenticated);
    }

    /// 実機確認用: `env -i HOME=$HOME SHELL=$SHELL <test-binary> --ignored --nocapture smoke`
    #[test]
    #[ignore]
    fn smoke_real_environment() {
        let info = collect_environment(&ShellEnv::resolve());
        println!("{info:#?}");
    }
}
