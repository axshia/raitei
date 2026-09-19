//! 永続化層（担当: WS-B）。
//!
//! 方針: SQLite（rusqlite, bundled）。DB ファイルは `<app_data_dir>/raitei.db`。
//! テーブル: `projects` / `tasks` / `agent_events`（詳細は docs/design.md）。
//!
//! 仮実装: プロセス内メモリ（Mutex<Vec>）。WS-B が SQLite 実装に置き換える。
//! 公開メソッドのシグネチャは契約として凍結。内部構造は自由に変更してよい。

use std::path::Path;
use std::sync::Mutex;

use crate::agent::types::{AgentEvent, AgentEventEnvelope};
use crate::error::{AppError, AppResult};
use crate::models::{now, AgentKind, Project, Task};

#[derive(Default)]
struct Mem {
    projects: Vec<Project>,
    tasks: Vec<Task>,
    events: Vec<AgentEventEnvelope>,
}

pub struct Store {
    mem: Mutex<Mem>,
}

impl Store {
    /// DB ファイルを開く（なければ作成しマイグレーション）。
    pub fn open(_path: &Path) -> AppResult<Self> {
        Ok(Store {
            mem: Mutex::new(Mem::default()),
        })
    }

    /// テスト用のインメモリ DB。
    pub fn open_in_memory() -> AppResult<Self> {
        Self::open(Path::new(":memory:"))
    }

    // ---- projects ----

    pub fn list_projects(&self) -> AppResult<Vec<Project>> {
        Ok(self.mem.lock().unwrap().projects.clone())
    }

    pub fn get_project(&self, id: &str) -> AppResult<Project> {
        self.mem
            .lock()
            .unwrap()
            .projects
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("project {id}")))
    }

    /// repo_path が重複する場合は `AppError::InvalidInput`。
    pub fn insert_project(&self, p: &Project) -> AppResult<()> {
        let mut m = self.mem.lock().unwrap();
        if m.projects.iter().any(|x| x.repo_path == p.repo_path) {
            return Err(AppError::InvalidInput(format!("登録済みです: {}", p.repo_path)));
        }
        m.projects.push(p.clone());
        Ok(())
    }

    /// プロジェクトと配下のタスク・イベントを削除する（ファイルシステムには触れない）。
    pub fn delete_project(&self, id: &str) -> AppResult<()> {
        let mut m = self.mem.lock().unwrap();
        let task_ids: Vec<String> = m.tasks.iter().filter(|t| t.project_id == id).map(|t| t.id.clone()).collect();
        m.projects.retain(|p| p.id != id);
        m.tasks.retain(|t| t.project_id != id);
        m.events.retain(|e| !task_ids.contains(&e.task_id));
        Ok(())
    }

    // ---- tasks ----

    pub fn list_tasks(&self, project_id: &str) -> AppResult<Vec<Task>> {
        Ok(self
            .mem
            .lock()
            .unwrap()
            .tasks
            .iter()
            .filter(|t| t.project_id == project_id)
            .cloned()
            .collect())
    }

    pub fn get_task(&self, id: &str) -> AppResult<Task> {
        self.mem
            .lock()
            .unwrap()
            .tasks
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("task {id}")))
    }

    pub fn insert_task(&self, t: &Task) -> AppResult<()> {
        self.mem.lock().unwrap().tasks.push(t.clone());
        Ok(())
    }

    /// id 一致の行を丸ごと置き換える。updated_at は呼び出し側で設定する。
    pub fn update_task(&self, t: &Task) -> AppResult<()> {
        let mut m = self.mem.lock().unwrap();
        let slot = m
            .tasks
            .iter_mut()
            .find(|x| x.id == t.id)
            .ok_or_else(|| AppError::NotFound(format!("task {}", t.id)))?;
        *slot = t.clone();
        Ok(())
    }

    /// タスクと配下のイベントを削除する。
    pub fn delete_task(&self, id: &str) -> AppResult<()> {
        let mut m = self.mem.lock().unwrap();
        m.tasks.retain(|t| t.id != id);
        m.events.retain(|e| e.task_id != id);
        Ok(())
    }

    /// エージェントのセッション ID を保存（None でリセット）。
    pub fn set_task_agent_session(&self, task_id: &str, session_id: Option<&str>) -> AppResult<()> {
        let mut t = self.get_task(task_id)?;
        t.agent_session_id = session_id.map(String::from);
        t.updated_at = now();
        self.update_task(&t)
    }

    pub fn set_task_pr_number(&self, task_id: &str, pr_number: Option<u64>) -> AppResult<()> {
        let mut t = self.get_task(task_id)?;
        t.pr_number = pr_number;
        t.updated_at = now();
        self.update_task(&t)
    }

    // ---- agent events ----

    /// イベントを追記し、seq（タスク内で 1 から単調増加）と timestamp を採番したエンベロープを返す。
    pub fn append_agent_event(
        &self,
        task_id: &str,
        run_id: &str,
        agent: AgentKind,
        event: AgentEvent,
    ) -> AppResult<AgentEventEnvelope> {
        let mut m = self.mem.lock().unwrap();
        let seq = m.events.iter().filter(|e| e.task_id == task_id).map(|e| e.seq).max().unwrap_or(0) + 1;
        let env = AgentEventEnvelope {
            task_id: task_id.to_string(),
            run_id: run_id.to_string(),
            agent,
            seq,
            timestamp: now(),
            event,
        };
        m.events.push(env.clone());
        Ok(env)
    }

    /// seq 昇順。`after_seq` 指定時はそれより大きいものだけ。
    pub fn list_agent_events(&self, task_id: &str, after_seq: Option<u64>) -> AppResult<Vec<AgentEventEnvelope>> {
        let m = self.mem.lock().unwrap();
        let mut v: Vec<_> = m
            .events
            .iter()
            .filter(|e| e.task_id == task_id && e.seq > after_seq.unwrap_or(0))
            .cloned()
            .collect();
        v.sort_by_key(|e| e.seq);
        Ok(v)
    }

    pub fn clear_agent_events(&self, task_id: &str) -> AppResult<()> {
        self.mem.lock().unwrap().events.retain(|e| e.task_id != task_id);
        Ok(())
    }
}
