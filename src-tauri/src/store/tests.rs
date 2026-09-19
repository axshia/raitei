use super::*;
use crate::models::new_id;

fn project(repo_path: &str) -> Project {
    Project {
        id: new_id(),
        name: "app".into(),
        repo_path: repo_path.into(),
        default_branch: "main".into(),
        created_at: now(),
    }
}

fn task(project_id: &str, branch: &str) -> Task {
    let ts = now();
    Task {
        id: new_id(),
        project_id: project_id.into(),
        title: format!("task {branch}"),
        branch: branch.into(),
        base_branch: "main".into(),
        worktree_path: format!("/src/app.worktrees/{}", branch.replace('/', "-")),
        agent: AgentKind::Claude,
        permission: PermissionLevel::Safe,
        agent_session_id: None,
        pr_number: None,
        created_at: ts.clone(),
        updated_at: ts,
    }
}

fn text(s: &str) -> AgentEvent {
    AgentEvent::AssistantText { text: s.into() }
}

#[test]
fn project_crud() {
    let s = Store::open_in_memory().unwrap();
    assert!(s.list_projects().unwrap().is_empty());
    let a = project("/src/a");
    let b = project("/src/b");
    s.insert_project(&a).unwrap();
    s.insert_project(&b).unwrap();
    assert_eq!(s.list_projects().unwrap(), vec![a.clone(), b.clone()]);
    assert_eq!(s.get_project(&a.id).unwrap(), a);
    s.delete_project(&a.id).unwrap();
    assert!(matches!(s.get_project(&a.id), Err(AppError::NotFound(_))));
    assert_eq!(s.list_projects().unwrap(), vec![b]);
    // 存在しない id の削除はエラーにしない
    s.delete_project("nope").unwrap();
}

#[test]
fn duplicate_repo_path_is_invalid_input() {
    let s = Store::open_in_memory().unwrap();
    s.insert_project(&project("/src/a")).unwrap();
    assert!(matches!(s.insert_project(&project("/src/a")), Err(AppError::InvalidInput(_))));
}

#[test]
fn task_crud_and_ordering() {
    let s = Store::open_in_memory().unwrap();
    let p = project("/src/a");
    s.insert_project(&p).unwrap();
    let mut t1 = task(&p.id, "feat/one");
    t1.pr_number = Some(42);
    t1.agent_session_id = Some("sess".into());
    let mut t2 = task(&p.id, "feat/two");
    t2.agent = AgentKind::Codex;
    t2.permission = PermissionLevel::Full;
    s.insert_task(&t1).unwrap();
    s.insert_task(&t2).unwrap();
    assert_eq!(s.list_tasks(&p.id).unwrap(), vec![t1.clone(), t2.clone()]);
    assert_eq!(s.get_task(&t2.id).unwrap(), t2);
    assert!(s.list_tasks("other").unwrap().is_empty());

    let mut u = t1.clone();
    u.title = "renamed".into();
    u.agent = AgentKind::Codex;
    u.agent_session_id = None;
    u.updated_at = now();
    s.update_task(&u).unwrap();
    assert_eq!(s.get_task(&t1.id).unwrap(), u);

    s.delete_task(&t1.id).unwrap();
    assert!(matches!(s.get_task(&t1.id), Err(AppError::NotFound(_))));
    assert_eq!(s.list_tasks(&p.id).unwrap(), vec![t2]);
}

#[test]
fn insert_task_errors() {
    let s = Store::open_in_memory().unwrap();
    // 親プロジェクトなし
    assert!(matches!(s.insert_task(&task("missing", "x")), Err(AppError::NotFound(_))));
    let p = project("/src/a");
    s.insert_project(&p).unwrap();
    s.insert_task(&task(&p.id, "x")).unwrap();
    // 同一プロジェクト内で同じブランチ
    assert!(matches!(s.insert_task(&task(&p.id, "x")), Err(AppError::InvalidInput(_))));
    // 別プロジェクトなら同じブランチ名でもよい
    let q = project("/src/b");
    s.insert_project(&q).unwrap();
    s.insert_task(&task(&q.id, "x")).unwrap();
}

#[test]
fn update_missing_task_is_not_found() {
    let s = Store::open_in_memory().unwrap();
    let t = task("p", "x");
    assert!(matches!(s.update_task(&t), Err(AppError::NotFound(_))));
    assert!(matches!(s.set_task_agent_session("nope", Some("s")), Err(AppError::NotFound(_))));
    assert!(matches!(s.set_task_pr_number("nope", Some(1)), Err(AppError::NotFound(_))));
}

#[test]
fn set_session_and_pr_number() {
    let s = Store::open_in_memory().unwrap();
    let p = project("/src/a");
    s.insert_project(&p).unwrap();
    let t = task(&p.id, "x");
    s.insert_task(&t).unwrap();

    s.set_task_agent_session(&t.id, Some("abc")).unwrap();
    s.set_task_pr_number(&t.id, Some(7)).unwrap();
    let got = s.get_task(&t.id).unwrap();
    assert_eq!(got.agent_session_id.as_deref(), Some("abc"));
    assert_eq!(got.pr_number, Some(7));
    assert!(got.updated_at >= t.updated_at);

    s.set_task_agent_session(&t.id, None).unwrap();
    s.set_task_pr_number(&t.id, None).unwrap();
    let got = s.get_task(&t.id).unwrap();
    assert_eq!(got.agent_session_id, None);
    assert_eq!(got.pr_number, None);
}

#[test]
fn agent_events_seq_and_roundtrip() {
    let s = Store::open_in_memory().unwrap();
    let p = project("/src/a");
    s.insert_project(&p).unwrap();
    let t1 = task(&p.id, "one");
    let t2 = task(&p.id, "two");
    s.insert_task(&t1).unwrap();
    s.insert_task(&t2).unwrap();

    let events = vec![
        AgentEvent::UserMessage { text: "hi".into() },
        AgentEvent::SessionStarted { session_id: "s".into(), model: Some("m".into()) },
        AgentEvent::ToolUse { id: "t".into(), name: "Bash".into(), input: serde_json::json!({"command": "ls"}) },
        AgentEvent::Result {
            is_error: false,
            text: None,
            duration_ms: Some(12),
            cost_usd: Some(0.5),
            usage: Some(serde_json::json!({"input_tokens": 3})),
        },
        AgentEvent::RunFinished { exit_code: Some(0), cancelled: false },
    ];
    let mut envs = Vec::new();
    for e in &events {
        envs.push(s.append_agent_event(&t1.id, "run1", AgentKind::Claude, e.clone()).unwrap());
    }
    assert_eq!(envs.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
    // seq はタスクごとに独立
    let other = s.append_agent_event(&t2.id, "run2", AgentKind::Codex, text("x")).unwrap();
    assert_eq!(other.seq, 1);

    let all = s.list_agent_events(&t1.id, None).unwrap();
    assert_eq!(all, envs);
    let after = s.list_agent_events(&t1.id, Some(3)).unwrap();
    assert_eq!(after.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![4, 5]);
    assert!(s.list_agent_events(&t1.id, Some(5)).unwrap().is_empty());

    s.clear_agent_events(&t1.id).unwrap();
    assert!(s.list_agent_events(&t1.id, None).unwrap().is_empty());
    assert_eq!(s.list_agent_events(&t2.id, None).unwrap().len(), 1);
    // クリア後は 1 から採番し直す
    assert_eq!(s.append_agent_event(&t1.id, "run3", AgentKind::Claude, text("y")).unwrap().seq, 1);
}

#[test]
fn append_event_for_missing_task_is_not_found() {
    let s = Store::open_in_memory().unwrap();
    assert!(matches!(
        s.append_agent_event("nope", "r", AgentKind::Claude, text("x")),
        Err(AppError::NotFound(_))
    ));
}

#[test]
fn deletes_cascade() {
    let s = Store::open_in_memory().unwrap();
    let p = project("/src/a");
    s.insert_project(&p).unwrap();
    let t1 = task(&p.id, "one");
    let t2 = task(&p.id, "two");
    s.insert_task(&t1).unwrap();
    s.insert_task(&t2).unwrap();
    s.append_agent_event(&t1.id, "r", AgentKind::Claude, text("a")).unwrap();
    s.append_agent_event(&t2.id, "r", AgentKind::Claude, text("b")).unwrap();

    s.delete_task(&t1.id).unwrap();
    assert!(s.list_agent_events(&t1.id, None).unwrap().is_empty());
    assert_eq!(s.list_agent_events(&t2.id, None).unwrap().len(), 1);

    s.delete_project(&p.id).unwrap();
    assert!(s.list_tasks(&p.id).unwrap().is_empty());
    assert!(s.list_agent_events(&t2.id, None).unwrap().is_empty());
}

#[test]
fn file_db_persists_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    // 親ディレクトリが無くても作る
    let path = dir.path().join("nested").join("raitei.db");
    let p = project("/src/a");
    let t = task(&p.id, "one");
    {
        let s = Store::open(&path).unwrap();
        s.insert_project(&p).unwrap();
        s.insert_task(&t).unwrap();
        s.append_agent_event(&t.id, "r", AgentKind::Claude, text("persisted")).unwrap();
    }
    let s = Store::open(&path).unwrap();
    assert_eq!(s.list_projects().unwrap(), vec![p]);
    assert_eq!(s.get_task(&t.id).unwrap(), t);
    let ev = s.list_agent_events(&t.id, None).unwrap();
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].event, text("persisted"));
    // 続きの seq から採番される
    assert_eq!(s.append_agent_event(&t.id, "r", AgentKind::Claude, text("next")).unwrap().seq, 2);
}

#[test]
fn store_is_usable_across_threads() {
    use std::sync::Arc;
    let s = Arc::new(Store::open_in_memory().unwrap());
    let p = project("/src/a");
    s.insert_project(&p).unwrap();
    let t = task(&p.id, "one");
    s.insert_task(&t).unwrap();
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let s = s.clone();
            let id = t.id.clone();
            std::thread::spawn(move || {
                for j in 0..25 {
                    s.append_agent_event(&id, "r", AgentKind::Claude, text(&format!("{i}-{j}"))).unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let seqs: Vec<u64> = s.list_agent_events(&t.id, None).unwrap().iter().map(|e| e.seq).collect();
    assert_eq!(seqs, (1..=100).collect::<Vec<_>>());
}
