//! スキーママイグレーション（担当: WS-B）。
//!
//! バージョンは `PRAGMA user_version` で管理する。`MIGRATIONS[i]` を適用すると
//! user_version が `i + 1` になる。既存のマイグレーションは書き換えず、変更は末尾に追加すること。

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

/// 順番に適用する DDL。各要素が 1 バージョン分。
const MIGRATIONS: &[&str] = &[
    // v1: 初期スキーマ（docs/design.md §4.4）
    r#"
    CREATE TABLE projects (
      id             TEXT PRIMARY KEY,
      name           TEXT NOT NULL,
      repo_path      TEXT NOT NULL UNIQUE,
      default_branch TEXT NOT NULL,
      created_at     TEXT NOT NULL
    );
    CREATE TABLE tasks (
      id               TEXT PRIMARY KEY,
      project_id       TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
      title            TEXT NOT NULL,
      branch           TEXT NOT NULL,
      base_branch      TEXT NOT NULL,
      worktree_path    TEXT NOT NULL,
      agent            TEXT NOT NULL,
      permission       TEXT NOT NULL,
      agent_session_id TEXT,
      pr_number        INTEGER,
      created_at       TEXT NOT NULL,
      updated_at       TEXT NOT NULL
    );
    CREATE INDEX idx_tasks_project ON tasks(project_id);
    CREATE UNIQUE INDEX idx_tasks_project_branch ON tasks(project_id, branch);
    CREATE TABLE agent_events (
      task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
      seq        INTEGER NOT NULL,
      run_id     TEXT NOT NULL,
      agent      TEXT NOT NULL,
      timestamp  TEXT NOT NULL,
      event_json TEXT NOT NULL,
      PRIMARY KEY (task_id, seq)
    );
    "#,
];

/// 現在のコードが扱える最新スキーマバージョン。
pub const LATEST_VERSION: i64 = MIGRATIONS.len() as i64;

pub fn user_version(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

/// 未適用のマイグレーションを 1 バージョンずつトランザクションで適用する。
/// DB が新しいバージョンのアプリで作られていた場合は `AppError::Db`。
pub fn migrate(conn: &mut Connection) -> AppResult<()> {
    let current = user_version(conn)?;
    if current > LATEST_VERSION {
        return Err(AppError::Db(format!(
            "DB のスキーマ (v{current}) がこのアプリ (v{LATEST_VERSION}) より新しいため開けません"
        )));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let version = i as i64 + 1;
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        // PRAGMA はバインド変数を受け付けないため format で埋め込む（値は内部定数）。
        tx.execute_batch(&format!("PRAGMA user_version = {version}"))?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_from_empty_and_idempotent() {
        let mut c = Connection::open_in_memory().unwrap();
        assert_eq!(user_version(&c).unwrap(), 0);
        migrate(&mut c).unwrap();
        assert_eq!(user_version(&c).unwrap(), LATEST_VERSION);
        // 2 回目は何もしない
        migrate(&mut c).unwrap();
        assert_eq!(user_version(&c).unwrap(), LATEST_VERSION);
        let n: i64 = c
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('projects','tasks','agent_events')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 3);
    }

    #[test]
    fn newer_schema_is_rejected() {
        let mut c = Connection::open_in_memory().unwrap();
        c.execute_batch(&format!("PRAGMA user_version = {}", LATEST_VERSION + 1)).unwrap();
        assert!(matches!(migrate(&mut c), Err(AppError::Db(_))));
    }
}
