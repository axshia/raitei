//! 実機の CLI を使う確認（既定では実行しない）。
//!
//! ```sh
//! cd src-tauri
//! cargo test --test real_cli -- --ignored --nocapture --test-threads=1
//! ```
//!
//! - `real_claude_*` / `real_codex_*`: ログインシェルの PATH にある claude / codex を IPC 経由で呼び、
//!   正規化イベントの保存・配信、session_id の保存と resume による会話継続、会話リセットを確かめる。
//!   API 利用料がかかる（短いプロンプトを 2 回ずつ送る）
//! - `real_gh_*`: 実 GitHub（cli/cli の公開 PR）に対する読み取り専用の `gh pr view` / `gh pr list` だけを使う。
//!   PR の作成・マージ・push など外部に見える操作はしない

mod common;

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};

use common::*;
use raitei_lib::agent::AgentEventEnvelope;

const RUN_TIMEOUT: Duration = Duration::from_secs(300);

fn setup(h: &Harness, agent: &str) -> (Value, PathBuf) {
    let repo = h.root().join("app");
    h.init_repo(&repo, &[("README.md", "# app\n")]);
    let project = h.ok("add_project", json!({ "path": repo }));
    let task = h.ok(
        "create_task",
        json!({ "req": {
            "projectId": project["id"], "title": "hello", "branch": format!("task/{agent}-hello"),
            "agent": agent, "permission": "safe",
        }}),
    );
    let wt = PathBuf::from(s(&task["worktreePath"]));
    (task, wt)
}

fn send(h: &Harness, task_id: &Value, text: &str) -> Vec<AgentEventEnvelope> {
    let info = h.ok("send_agent_message", json!({ "req": { "taskId": task_id, "text": text } }));
    let run_id = s(&info["runId"]).to_string();
    assert_eq!(info["taskId"], *task_id);
    let evs = h.sink.wait_finished(&run_id, RUN_TIMEOUT);
    println!("--- run {run_id}");
    for e in &evs {
        let v = serde_json::to_value(&e.event).unwrap();
        let mut line = v.to_string();
        if line.len() > 200 {
            let cut = (0..=200).rev().find(|i| line.is_char_boundary(*i)).unwrap();
            line.truncate(cut);
            line.push('…');
        }
        println!("{:>3} {line}", e.seq);
    }
    evs
}

fn types(evs: &[AgentEventEnvelope]) -> Vec<String> {
    evs.iter().map(|e| e.event_type()).collect()
}

fn session_id(evs: &[AgentEventEnvelope]) -> Option<String> {
    evs.iter().find_map(|e| {
        let v = serde_json::to_value(&e.event).unwrap();
        (v["type"] == "session_started").then(|| v["session_id"].as_str().unwrap().to_string())
    })
}

/// run の最後の本文（Result.text、無ければ最後の AssistantText）。
fn final_text(evs: &[AgentEventEnvelope]) -> String {
    let vals: Vec<Value> = evs.iter().map(|e| serde_json::to_value(&e.event).unwrap()).collect();
    vals.iter()
        .rev()
        .find_map(|v| (v["type"] == "result").then(|| v["text"].as_str().map(String::from)).flatten())
        .or_else(|| vals.iter().rev().find_map(|v| (v["type"] == "assistant_text").then(|| v["text"].as_str().unwrap().to_string())))
        .unwrap_or_default()
}

fn assert_completed_run(evs: &[AgentEventEnvelope]) {
    let t = types(evs);
    assert_eq!(t.first().map(String::as_str), Some("user_message"), "{t:?}");
    assert_eq!(t.last().map(String::as_str), Some("run_finished"), "{t:?}");
    assert_eq!(t.iter().filter(|x| *x == "run_finished").count(), 1);
    assert!(t.contains(&"session_started".into()), "{t:?}");
    assert!(t.contains(&"assistant_text".into()), "{t:?}");
    let result = evs
        .iter()
        .map(|e| serde_json::to_value(&e.event).unwrap())
        .find(|v| v["type"] == "result")
        .unwrap_or_else(|| panic!("result が無い: {t:?}"));
    assert_eq!(result["is_error"], false, "{result}");
    let finished = serde_json::to_value(&evs.last().unwrap().event).unwrap();
    assert_eq!(finished["exit_code"], 0, "{finished}");
    assert_eq!(finished["cancelled"], false);
    for w in evs.windows(2) {
        assert!(w[0].seq < w[1].seq, "seq が単調増加していない");
    }
}

fn agent_develops_and_resumes(agent: &str) {
    let h = Harness::with_login_shell_path();
    let (task, wt) = setup(&h, agent);
    let id = task["id"].clone();

    // 1 ターン目: ファイルを作らせる
    let first = send(
        &h,
        &id,
        "Create a new file named hello.txt in the current directory whose entire content is the single word raitei. \
         Do not run any git commands. When finished, reply with only the word DONE.",
    );
    assert_completed_run(&first);
    assert!(
        types(&first).iter().any(|t| t == "tool_use") && types(&first).iter().any(|t| t == "tool_result"),
        "ファイル作成のツール呼び出しが正規化されていない: {:?}",
        types(&first)
    );
    assert_eq!(read(&wt, "hello.txt").trim(), "raitei");
    let sid = session_id(&first).unwrap();
    let saved = h.ok("get_task", json!({ "taskId": id }));
    assert_eq!(saved["agentSessionId"], sid.as_str(), "session_id が Task に保存される");
    assert_eq!(h.ok("get_agent_run_state", json!({ "taskId": id }))["running"], false);
    let st = h.ok("get_git_status", json!({ "taskId": id }));
    assert!(st["files"].as_array().unwrap().iter().any(|f| f["path"] == "hello.txt"), "{st}");

    // 保存された履歴 = 配信されたイベント
    let history = h.ok("get_agent_history", json!({ "taskId": id, "afterSeq": null }));
    assert_eq!(history, serde_json::to_value(&first).unwrap());

    // 2 ターン目: resume で前のターンを覚えているか
    let second = send(
        &h,
        &id,
        "What is the exact name of the file you created in your previous turn? Reply with only the file name.",
    );
    assert_completed_run(&second);
    assert_eq!(session_id(&second).as_deref(), Some(sid.as_str()), "同じセッションで継続する");
    let answer = final_text(&second);
    assert!(answer.contains("hello.txt"), "前のターンを覚えていない: {answer:?}");

    let last_seq = first.last().unwrap().seq;
    let tail = h.ok("get_agent_history", json!({ "taskId": id, "afterSeq": last_seq }));
    assert_eq!(tail, serde_json::to_value(&second).unwrap(), "afterSeq 以降だけ返す");

    // リセットで履歴と session_id を消す
    h.ok("reset_agent_session", json!({ "taskId": id }));
    assert_eq!(h.ok("get_agent_history", json!({ "taskId": id })), json!([]));
    assert_eq!(h.ok("get_task", json!({ "taskId": id }))["agentSessionId"], Value::Null);
}

#[test]
#[ignore = "実機の claude を呼ぶ（API 利用料がかかる）"]
fn real_claude_develops_and_resumes() {
    agent_develops_and_resumes("claude");
}

#[test]
#[ignore = "実機の codex を呼ぶ（API 利用料がかかる）"]
fn real_codex_develops_and_resumes() {
    agent_develops_and_resumes("codex");
}

fn gh_json(h: &Harness, args: &[&str]) -> Value {
    let out = raitei_lib::shell_env::run(&h.state.env, "gh", args, h.root()).unwrap();
    assert!(out.success(), "gh {args:?}: {}", out.stderr);
    serde_json::from_str(&out.stdout).unwrap()
}

/// 実 GitHub の公開リポジトリ（cli/cli）に対して、読み取りだけで PR 状態確認を通す。
#[test]
#[ignore = "実 GitHub に読み取り専用でアクセスする"]
fn real_gh_read_only_pull_request_status() {
    let h = Harness::with_login_shell_path();
    assert!(raitei_lib::github::gh::is_authenticated(&h.state.env), "gh auth status が失敗");

    // 同一リポジトリ内ブランチの merged PR を 1 つ選ぶ
    let list = gh_json(
        &h,
        &["pr", "list", "--repo", "cli/cli", "--state", "merged", "--limit", "40", "--json", "number,headRefName,isCrossRepository"],
    );
    let pick = list
        .as_array()
        .unwrap()
        .iter()
        .find(|p| {
            p["isCrossRepository"] == false && raitei_lib::git::repo::is_valid_branch_name(s(&p["headRefName"]))
        })
        .expect("同一リポジトリの merged PR が見つからない")
        .clone();
    let head = s(&pick["headRefName"]).to_string();
    println!("picked cli/cli#{} ({head})", pick["number"]);

    // origin が GitHub を指すローカルリポジトリ（fetch / push はしない）
    let repo = h.root().join("cli");
    h.init_repo(&repo, &[("README.md", "x\n")]);
    h.git(&repo, &["remote", "add", "origin", "https://github.com/cli/cli.git"]);
    let project = h.ok("add_project", json!({ "path": repo }));
    let task = h.ok(
        "create_task",
        json!({ "req": { "projectId": project["id"], "title": "", "branch": head, "agent": "claude" } }),
    );

    let pr = h.ok("get_pull_request", json!({ "taskId": task["id"] }));
    println!("{}", serde_json::to_string_pretty(&pr).unwrap());
    assert_eq!(pr["number"], pick["number"]);
    assert_eq!(pr["state"], "merged");
    assert_eq!(pr["headBranch"], head.as_str());
    assert_eq!(pr["checks"]["total"], pr["checks"]["checks"].as_array().unwrap().len());
    assert_eq!(h.ok("get_task", json!({ "taskId": task["id"] }))["prNumber"], pick["number"]);

    // PR の無いブランチは null（"no pull requests found" をエラー扱いしない）
    let none = h.ok(
        "create_task",
        json!({ "req": { "projectId": project["id"], "title": "", "branch": "raitei/no-such-branch-7c1e", "agent": "codex" } }),
    );
    assert_eq!(h.ok("get_pull_request", json!({ "taskId": none["id"] })), Value::Null);

    // open PR（CI チェック・レビュー付き）も同じフィールドで取得・パースできる
    let open = gh_json(&h, &["pr", "list", "--repo", "cli/cli", "--state", "open", "--limit", "5", "--json", "number"]);
    for p in open.as_array().unwrap() {
        let n = p["number"].as_u64().unwrap();
        let out = raitei_lib::shell_env::run(
            &h.state.env,
            "gh",
            &["pr", "view", &n.to_string(), "--repo", "cli/cli", "--json", raitei_lib::github::PR_JSON_FIELDS],
            h.root(),
        )
        .unwrap();
        assert!(out.success(), "{}", out.stderr);
        let parsed = raitei_lib::github::parse::parse_pr_view(&out.stdout).unwrap();
        println!(
            "cli/cli#{n}: state={:?} mergeable={:?} mergeState={} review={:?} checks={}/{} (failed {}, pending {})",
            parsed.state,
            parsed.mergeable,
            parsed.merge_state_status,
            parsed.review_decision,
            parsed.checks.passed,
            parsed.checks.total,
            parsed.checks.failed,
            parsed.checks.pending
        );
    }
}

/// 調査用: 安全モードのエージェントが AI コンフリクト解消（git add）とコミットをできるか。
fn probe_git_in_safe_mode(agent: &str) {
    let h = Harness::with_login_shell_path();
    let (task, wt) = setup(&h, agent);
    let id = task["id"].clone();
    let repo = h.root().join("app");
    write(&wt, "greet.txt", "hello from task\n");
    h.commit_all(&wt, "task greet");
    write(&repo, "greet.txt", "hello from main\n");
    h.commit_all(&repo, "main greet");
    let cs = h.ok("start_base_merge", json!({ "taskId": id }));
    assert_eq!(cs["files"].as_array().unwrap().len(), 1);

    let run = h.ok("request_agent_conflict_resolution", json!({ "taskId": id }));
    let evs = h.sink.wait_finished(s(&run["runId"]), RUN_TIMEOUT);
    println!("[{agent}] conflict run types: {:?}", types(&evs));
    for e in &evs {
        let v = serde_json::to_value(&e.event).unwrap();
        if matches!(v["type"].as_str(), Some("error" | "tool_use" | "tool_result" | "result")) {
            let mut line = v.to_string();
            line.truncate(line.char_indices().nth(300).map(|(i, _)| i).unwrap_or(line.len()));
            println!("  {line}");
        }
    }
    let cs = h.ok("get_conflict_state", json!({ "taskId": id }));
    println!("[{agent}] after AI: {cs}");
    println!("[{agent}] greet.txt: {:?}", read(&wt, "greet.txt"));

    // 解消できていなければ手動で解決済みにしてマージを終える
    if !cs["files"].as_array().unwrap().is_empty() {
        h.ok("resolve_conflict_file", json!({ "taskId": id, "path": "greet.txt", "resolution": "markResolved" }));
    }
    h.ok("complete_base_merge", json!({ "taskId": id, "push": false }));

    // 開発 → コミットまで頼めるか
    let before = h.git(&wt, &["rev-parse", "HEAD"]);
    let evs = send(
        &h,
        &id,
        "Create a file named feature.txt containing the word feature, then commit it with git using the message \
         'add feature'. Reply with only DONE or the reason you could not commit.",
    );
    let after = h.git(&wt, &["rev-parse", "HEAD"]);
    println!("[{agent}] committed: {} / final: {:?}", before != after, final_text(&evs));
    println!("[{agent}] status: {}", h.ok("get_git_status", json!({ "taskId": id }))["files"]);
}

#[test]
#[ignore = "調査用: 実機の claude を呼ぶ"]
fn real_claude_probe_git_in_safe_mode() {
    probe_git_in_safe_mode("claude");
}

#[test]
#[ignore = "調査用: 実機の codex を呼ぶ"]
fn real_codex_probe_git_in_safe_mode() {
    probe_git_in_safe_mode("codex");
}
