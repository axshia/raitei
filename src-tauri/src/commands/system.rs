//! 環境情報 command（担当: WS-A）。

use tauri::State;

use crate::error::AppResult;
use crate::github;
use crate::models::{EnvironmentInfo, ToolInfo};
use crate::state::{blocking, AppState};

/// 各 CLI の検出結果と gh 認証状態。
/// 仮実装: パス検出と gh 認証のみ。WS-A が `--version` 取得を追加する。
#[tauri::command]
pub async fn get_environment(state: State<'_, AppState>) -> AppResult<EnvironmentInfo> {
    let env = state.env.clone();
    blocking(move || {
        let tool = |name: &str| ToolInfo {
            name: name.to_string(),
            path: env.which(name).map(|p| p.display().to_string()),
            version: None,
        };
        Ok(EnvironmentInfo {
            path: env.path.clone(),
            git: tool("git"),
            gh: tool("gh"),
            claude: tool("claude"),
            codex: tool("codex"),
            gh_authenticated: github::gh::is_authenticated(&env),
        })
    })
    .await
}
