//! 永続化層（担当: WS-B）。
//!
//! SQLite（rusqlite, bundled）。DB ファイルは `<app_data_dir>/raitei.db`。
//! テーブル: `projects` / `tasks` / `agent_events`（DDL は [`migrations`]、詳細は docs/design.md §4.4）。
//!
//! - スキーマは `PRAGMA user_version` で管理し、`open` 時に未適用分を適用する
//! - 接続は 1 本を `Mutex<Connection>` で共有する（書き込み頻度が低いため十分）
//! - 外部キーは有効化しており、プロジェクト削除でタスク・イベントが連鎖削除される
//!
//! 公開メソッドのシグネチャは契約として凍結。

mod migrations;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::{ffi, params, Connection, OptionalExtension, Row};

use crate::agent::types::{AgentEvent, AgentEventEnvelope};
use crate::error::{AppError, AppResult};
use crate::models::{now, AgentKind, PermissionLevel, Project, Task};

pub struct Store {
    conn: Mutex<Connection>,
}

const TASK_COLUMNS: &str = "id, project_id, title, branch, base_branch, worktree_path, agent, permission, \
     agent_session_id, pr_number, created_at, updated_at";

impl Store {
    /// DB ファイルを開く（なければ作成しマイグレーション）。`:memory:` ならインメモリ DB。
    pub fn open(path: &Path) -> AppResult<Self> {
        let conn = if path == Path::new(":memory:") {
            Connection::open_in_memory()?
        } else {
            if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir)?;
            }
            let c = Connection::open(path)?;
            // WAL は読み書きの並行性が高く、クラッシュ時も壊れにくい。
            c.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
            c
        };
        Self::init(conn)
    }

    /// テスト用のインメモリ DB。
    pub fn open_in_memory() -> AppResult<Self> {
        Self::open(Path::new(":memory:"))
    }

    fn init(mut conn: Connection) -> AppResult<Self> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrations::migrate(&mut conn)?;
        Ok(Store { conn: Mutex::new(conn) })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        // 別スレッドが panic しても接続自体は使えるので poison は無視する。
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---- projects ----

    /// 登録順（created_at 昇順）。
    pub fn list_projects(&self) -> AppResult<Vec<Project>> {
        let c = self.conn();
        let mut st = c.prepare(
            "SELECT id, name, repo_path, default_branch, created_at FROM projects ORDER BY created_at, rowid",
        )?;
        let rows = st.query_map([], row_to_project)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_project(&self, id: &str) -> AppResult<Project> {
        self.conn()
            .query_row(
                "SELECT id, name, repo_path, default_branch, created_at FROM projects WHERE id = ?1",
                [id],
                row_to_project,
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("project {id}")))
    }

    /// repo_path が重複する場合は `AppError::InvalidInput`。
    pub fn insert_project(&self, p: &Project) -> AppResult<()> {
        self.conn()
            .execute(
                "INSERT INTO projects (id, name, repo_path, default_branch, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![p.id, p.name, p.repo_path, p.default_branch, p.created_at],
            )
            .map_err(|e| match constraint_code(&e) {
                Some(ffi::SQLITE_CONSTRAINT_UNIQUE) => {
                    AppError::InvalidInput(format!("登録済みです: {}", p.repo_path))
                }
                Some(ffi::SQLITE_CONSTRAINT_PRIMARYKEY) => {
                    AppError::InvalidInput(format!("プロジェクト ID が重複しています: {}", p.id))
                }
                _ => e.into(),
            })?;
        Ok(())
    }

    /// プロジェクトと配下のタスク・イベントを削除する（ファイルシステムには触れない）。
    /// 存在しない id でもエラーにしない。
    pub fn delete_project(&self, id: &str) -> AppResult<()> {
        // tasks / agent_events は ON DELETE CASCADE で消える。
        self.conn().execute("DELETE FROM projects WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---- tasks ----

    /// 作成順（created_at 昇順）。
    pub fn list_tasks(&self, project_id: &str) -> AppResult<Vec<Task>> {
        let c = self.conn();
        let mut st = c.prepare(&format!(
            "SELECT {TASK_COLUMNS} FROM tasks WHERE project_id = ?1 ORDER BY created_at, rowid"
        ))?;
        let rows = st.query_map([project_id], row_to_task)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_task(&self, id: &str) -> AppResult<Task> {
        self.conn()
            .query_row(&format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"), [id], row_to_task)
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("task {id}")))
    }

    /// プロジェクトが存在しなければ `NotFound`、同一プロジェクトに同じブランチのタスクがあれば `InvalidInput`。
    pub fn insert_task(&self, t: &Task) -> AppResult<()> {
        self.conn()
            .execute(
                &format!(
                    "INSERT INTO tasks ({TASK_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
                ),
                params![
                    t.id,
                    t.project_id,
                    t.title,
                    t.branch,
                    t.base_branch,
                    t.worktree_path,
                    t.agent.as_str(),
                    permission_str(t.permission),
                    t.agent_session_id,
                    t.pr_number.map(|n| n as i64),
                    t.created_at,
                    t.updated_at,
                ],
            )
            .map_err(|e| match constraint_code(&e) {
                Some(ffi::SQLITE_CONSTRAINT_FOREIGNKEY) => AppError::NotFound(format!("project {}", t.project_id)),
                Some(ffi::SQLITE_CONSTRAINT_UNIQUE) => {
                    AppError::InvalidInput(format!("ブランチ {} のタスクは既にあります", t.branch))
                }
                Some(ffi::SQLITE_CONSTRAINT_PRIMARYKEY) => {
                    AppError::InvalidInput(format!("タスク ID が重複しています: {}", t.id))
                }
                _ => e.into(),
            })?;
        Ok(())
    }

    /// id 一致の行を丸ごと置き換える。updated_at は呼び出し側で設定する。
    /// `project_id` / `created_at` は変更しない。
    pub fn update_task(&self, t: &Task) -> AppResult<()> {
        let n = self
            .conn()
            .execute(
                "UPDATE tasks SET title = ?2, branch = ?3, base_branch = ?4, worktree_path = ?5, agent = ?6, \
                 permission = ?7, agent_session_id = ?8, pr_number = ?9, updated_at = ?10 WHERE id = ?1",
                params![
                    t.id,
                    t.title,
                    t.branch,
                    t.base_branch,
                    t.worktree_path,
                    t.agent.as_str(),
                    permission_str(t.permission),
                    t.agent_session_id,
                    t.pr_number.map(|n| n as i64),
                    t.updated_at,
                ],
            )
            .map_err(|e| match constraint_code(&e) {
                Some(ffi::SQLITE_CONSTRAINT_UNIQUE) => {
                    AppError::InvalidInput(format!("ブランチ {} のタスクは既にあります", t.branch))
                }
                _ => e.into(),
            })?;
        if n == 0 {
            return Err(AppError::NotFound(format!("task {}", t.id)));
        }
        Ok(())
    }

    /// タスクと配下のイベントを削除する。存在しない id でもエラーにしない。
    pub fn delete_task(&self, id: &str) -> AppResult<()> {
        self.conn().execute("DELETE FROM tasks WHERE id = ?1", [id])?;
        Ok(())
    }

    /// エージェントのセッション ID を保存（None でリセット）。
    pub fn set_task_agent_session(&self, task_id: &str, session_id: Option<&str>) -> AppResult<()> {
        let n = self.conn().execute(
            "UPDATE tasks SET agent_session_id = ?2, updated_at = ?3 WHERE id = ?1",
            params![task_id, session_id, now()],
        )?;
        if n == 0 {
            return Err(AppError::NotFound(format!("task {task_id}")));
        }
        Ok(())
    }

    pub fn set_task_pr_number(&self, task_id: &str, pr_number: Option<u64>) -> AppResult<()> {
        let n = self.conn().execute(
            "UPDATE tasks SET pr_number = ?2, updated_at = ?3 WHERE id = ?1",
            params![task_id, pr_number.map(|n| n as i64), now()],
        )?;
        if n == 0 {
            return Err(AppError::NotFound(format!("task {task_id}")));
        }
        Ok(())
    }

    // ---- agent events ----

    /// イベントを追記し、seq（タスク内で 1 から単調増加）と timestamp を採番したエンベロープを返す。
    /// タスクが存在しなければ `NotFound`。
    pub fn append_agent_event(
        &self,
        task_id: &str,
        run_id: &str,
        agent: AgentKind,
        event: AgentEvent,
    ) -> AppResult<AgentEventEnvelope> {
        let event_json = serde_json::to_string(&event)?;
        let mut c = self.conn();
        // Mutex で直列化されているが、採番と挿入を 1 トランザクションにしておく。
        let tx = c.transaction()?;
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM agent_events WHERE task_id = ?1",
            [task_id],
            |r| r.get(0),
        )?;
        let timestamp = now();
        tx.execute(
            "INSERT INTO agent_events (task_id, seq, run_id, agent, timestamp, event_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![task_id, seq, run_id, agent.as_str(), timestamp, event_json],
        )
        .map_err(|e| match constraint_code(&e) {
            Some(ffi::SQLITE_CONSTRAINT_FOREIGNKEY) => AppError::NotFound(format!("task {task_id}")),
            _ => e.into(),
        })?;
        tx.commit()?;
        Ok(AgentEventEnvelope {
            task_id: task_id.to_string(),
            run_id: run_id.to_string(),
            agent,
            seq: seq as u64,
            timestamp,
            event,
        })
    }

    /// seq 昇順。`after_seq` 指定時はそれより大きいものだけ。
    pub fn list_agent_events(&self, task_id: &str, after_seq: Option<u64>) -> AppResult<Vec<AgentEventEnvelope>> {
        let c = self.conn();
        let mut st = c.prepare(
            "SELECT task_id, run_id, agent, seq, timestamp, event_json FROM agent_events \
             WHERE task_id = ?1 AND seq > ?2 ORDER BY seq",
        )?;
        let rows = st.query_map(params![task_id, after_seq.unwrap_or(0) as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (task_id, run_id, agent, seq, timestamp, json) = row?;
            out.push(AgentEventEnvelope {
                task_id,
                run_id,
                agent: parse_agent(&agent)?,
                seq: seq as u64,
                timestamp,
                event: serde_json::from_str(&json)
                    .map_err(|e| AppError::Db(format!("イベント seq={seq} を復元できません: {e}")))?,
            });
        }
        Ok(out)
    }

    pub fn clear_agent_events(&self, task_id: &str) -> AppResult<()> {
        self.conn().execute("DELETE FROM agent_events WHERE task_id = ?1", [task_id])?;
        Ok(())
    }
}

// ---- 行変換・値変換 ----

fn row_to_project(r: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        name: r.get(1)?,
        repo_path: r.get(2)?,
        default_branch: r.get(3)?,
        created_at: r.get(4)?,
    })
}

fn row_to_task(r: &Row<'_>) -> rusqlite::Result<Task> {
    let agent: String = r.get(6)?;
    let permission: String = r.get(7)?;
    let pr_number: Option<i64> = r.get(9)?;
    Ok(Task {
        id: r.get(0)?,
        project_id: r.get(1)?,
        title: r.get(2)?,
        branch: r.get(3)?,
        base_branch: r.get(4)?,
        worktree_path: r.get(5)?,
        agent: AgentKind::parse(&agent).ok_or_else(|| invalid_column(6, "agent", &agent))?,
        permission: parse_permission(&permission).ok_or_else(|| invalid_column(7, "permission", &permission))?,
        agent_session_id: r.get(8)?,
        pr_number: pr_number.map(|n| n as u64),
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
    })
}

fn invalid_column(idx: usize, name: &str, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        idx,
        rusqlite::types::Type::Text,
        format!("{name} の値が不正です: {value}").into(),
    )
}

fn parse_agent(s: &str) -> AppResult<AgentKind> {
    AgentKind::parse(s).ok_or_else(|| AppError::Db(format!("agent の値が不正です: {s}")))
}

fn permission_str(p: PermissionLevel) -> &'static str {
    match p {
        PermissionLevel::Safe => "safe",
        PermissionLevel::Full => "full",
    }
}

fn parse_permission(s: &str) -> Option<PermissionLevel> {
    match s {
        "safe" => Some(PermissionLevel::Safe),
        "full" => Some(PermissionLevel::Full),
        _ => None,
    }
}

/// 制約違反なら拡張エラーコード（`SQLITE_CONSTRAINT_*`）を返す。
fn constraint_code(e: &rusqlite::Error) -> Option<i32> {
    match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
            Some(f.extended_code)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
