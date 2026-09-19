//! MVP 要件を、フロントと同じ command 名・引数で Tauri の IPC 経由に確かめる結合テスト。
//!
//! - (1) プロジェクト作成・登録
//! - (2) 1 プロジェクトで複数 worktree（タスク）を並列に扱う
//! - (6) base 取り込みとコンフリクト検出・ours / theirs 解消・中止
//! - (4)(5)(7) PR 系 command の引数組み立てとパース（偽の gh とローカルの bare リモートを使う）
//!
//! GitHub への書き込みは行わない。push 先はすべて一時ディレクトリの bare リポジトリ。
//! 実機の claude / codex / gh を使う確認は `tests/real_cli.rs`（`--ignored`）にある。

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use common::*;
use common::EventType;
use raitei_lib::github::PR_JSON_FIELDS;

fn path_of(v: &Value, key: &str) -> PathBuf {
    PathBuf::from(s(&v[key]))
}

fn create_task(h: &Harness, project: &Value, branch: &str, agent: &str) -> Value {
    h.ok(
        "create_task",
        json!({ "req": {
            "projectId": project["id"], "title": format!("{branch} の作業"), "branch": branch,
            "baseBranch": null, "agent": agent, "permission": "safe",
        }}),
    )
}

#[test]
fn projects_and_parallel_worktree_tasks() {
    let h = Harness::new();

    // (1) 新規作成: git init -b main + 初回コミット
    std::fs::create_dir_all(h.root().join("src")).unwrap();
    let fresh = h.ok("create_project", json!({ "req": { "parentDir": h.root().join("src"), "name": "fresh" } }));
    assert_eq!(fresh["name"], "fresh");
    assert_eq!(fresh["defaultBranch"], "main");
    let fresh_path = path_of(&fresh, "repoPath");
    assert_eq!(h.git(&fresh_path, &["log", "-1", "--format=%s"]).trim(), "Initial commit");
    h.err(
        "create_project",
        json!({ "req": { "parentDir": h.root().join("src"), "name": "fresh" } }),
        "invalidInput",
    );

    // (1) 既存リポジトリの登録（サブディレクトリを渡してもトップレベルに正規化）
    let repo = h.root().join("app");
    h.init_repo(&repo, &[("README.md", "app\n"), ("sub/keep", "")]);
    let app = h.ok("add_project", json!({ "path": repo.join("sub") }));
    assert_eq!(app["name"], "app");
    assert_eq!(path_of(&app, "repoPath"), repo.canonicalize().unwrap());
    h.err("add_project", json!({ "path": repo }), "invalidInput");
    h.err("add_project", json!({ "path": h.root().join("nothing") }), "invalidInput");
    assert_eq!(h.ok("list_projects", json!({})).as_array().unwrap().len(), 2);

    // (2) 1 プロジェクトに claude / codex のタスクを並べて作る
    let t1 = create_task(&h, &app, "task/login", "claude");
    let t2 = h.ok(
        "create_task",
        // permission / baseBranch を省略しても作れる（フロントの型では任意）
        json!({ "req": { "projectId": app["id"], "title": "", "branch": "feat/api", "agent": "codex" } }),
    );
    assert_eq!(t2["title"], "feat/api", "空タイトルはブランチ名で補う");
    assert_eq!(t2["permission"], "safe");
    assert_eq!(t2["baseBranch"], "main");
    let (w1, w2) = (path_of(&t1, "worktreePath"), path_of(&t2, "worktreePath"));
    assert_ne!(w1, w2);
    assert_eq!(h.git(&w1, &["branch", "--show-current"]).trim(), "task/login");
    assert_eq!(h.git(&w2, &["branch", "--show-current"]).trim(), "feat/api");
    // 同じブランチ・base と同じブランチは作れない
    h.err(
        "create_task",
        json!({ "req": { "projectId": app["id"], "title": "x", "branch": "task/login", "agent": "claude" } }),
        "invalidInput",
    );
    h.err(
        "create_task",
        json!({ "req": { "projectId": app["id"], "title": "x", "branch": "main", "agent": "claude" } }),
        "invalidInput",
    );

    // worktree ごとに作業ツリーが独立している
    write(&w1, "login.txt", "login\n");
    let st1 = h.ok("get_git_status", json!({ "taskId": t1["id"] }));
    let st2 = h.ok("get_git_status", json!({ "taskId": t2["id"] }));
    assert_eq!(st1["branch"], "task/login");
    assert_eq!(st1["files"], json!([{ "path": "login.txt", "status": "??" }]));
    assert_eq!(st2["files"], json!([]));

    // worktree 一覧（main + 2 タスク）とタスクの対応
    let wts = h.ok("list_worktrees", json!({ "projectId": app["id"] }));
    let wts = wts.as_array().unwrap();
    assert_eq!(wts.len(), 3, "{wts:#?}");
    assert!(wts[0]["isMain"].as_bool().unwrap());
    for t in [&t1, &t2] {
        let w = wts.iter().find(|w| w["branch"] == t["branch"]).unwrap();
        assert_eq!(w["taskId"], t["id"], "{w}");
    }

    // (3) タブ表示の元になる一覧・取得・更新
    let tasks = h.ok("list_tasks", json!({ "projectId": app["id"] }));
    assert_eq!(tasks.as_array().unwrap().len(), 2);
    assert_eq!(h.ok("get_task", json!({ "taskId": t1["id"] }))["branch"], "task/login");
    let renamed = h.ok(
        "update_task",
        json!({ "req": { "taskId": t1["id"], "title": "新しい題", "permission": "full" } }),
    );
    assert_eq!(renamed["title"], "新しい題");
    assert_eq!(renamed["permission"], "full");
    assert_eq!(renamed["agent"], "claude");

    // 削除（worktree・ブランチも消す）
    h.ok(
        "delete_task",
        json!({ "taskId": t2["id"], "options": { "removeWorktree": true, "deleteBranch": true, "force": false } }),
    );
    assert!(!w2.exists());
    assert!(h.git(&repo, &["branch", "--list", "feat/api"]).trim().is_empty());
    assert_eq!(h.ok("list_tasks", json!({ "projectId": app["id"] })).as_array().unwrap().len(), 1);
    h.err("get_task", json!({ "taskId": t2["id"] }), "notFound");

    // 登録解除はファイルを消さない
    h.ok("remove_project", json!({ "projectId": fresh["id"] }));
    assert!(fresh_path.join(".git").is_dir());
    assert_eq!(h.ok("list_projects", json!({})).as_array().unwrap().len(), 1);
}

/// base（main）とタスクの両方で同じファイルを変え、取り込み → 検出 → ours / theirs → コミットまで。
#[test]
fn base_merge_detects_conflicts_and_resolves_with_ours_and_theirs() {
    let h = Harness::new();
    let repo = h.root().join("app");
    h.init_repo(&repo, &[("a.txt", "base\n"), ("b.txt", "base\n"), ("c.txt", "base\n")]);
    let app = h.ok("add_project", json!({ "path": repo }));
    let t = create_task(&h, &app, "task/conflict", "claude");
    let wt = path_of(&t, "worktreePath");
    let id = t["id"].clone();

    write(&wt, "a.txt", "task\n");
    write(&wt, "b.txt", "task\n");
    h.commit_all(&wt, "task changes");
    write(&repo, "a.txt", "main\n");
    write(&repo, "b.txt", "main\n");
    write(&repo, "c.txt", "main only\n");
    h.commit_all(&repo, "main changes");
    let task_head = h.git(&wt, &["rev-parse", "HEAD"]);

    // 未コミット変更があると取り込まない
    write(&wt, "dirty.txt", "x");
    h.err("start_base_merge", json!({ "taskId": id }), "invalidInput");
    std::fs::remove_file(wt.join("dirty.txt")).unwrap();

    let cs = h.ok("start_base_merge", json!({ "taskId": id }));
    assert_eq!(cs["mergeInProgress"], true);
    assert_eq!(cs["baseRef"], "main", "origin が無いのでローカルの main を取り込む");
    assert_eq!(
        cs["files"],
        json!([{ "path": "a.txt", "kind": "bothModified" }, { "path": "b.txt", "kind": "bothModified" }])
    );
    assert_eq!(cs["readyToCommit"], false);
    assert_eq!(h.ok("get_conflict_state", json!({ "taskId": id })), cs);
    let st = h.ok("get_git_status", json!({ "taskId": id }));
    assert_eq!(st["mergeInProgress"], true);

    let content = h.ok("read_conflict_file", json!({ "taskId": id, "path": "a.txt" }));
    assert!(s(&content["working"]).contains("<<<<<<<"), "{content}");
    assert_eq!(content["ours"], "task\n");
    assert_eq!(content["theirs"], "main\n");
    h.err("read_conflict_file", json!({ "taskId": id, "path": "../a.txt" }), "invalidInput");
    h.err("read_conflict_file", json!({ "taskId": id, "path": "c.txt" }), "invalidInput");

    // コミットは未解決が残っていると拒否
    h.err("complete_base_merge", json!({ "taskId": id, "push": false }), "invalidInput");

    let cs = h.ok("resolve_conflict_file", json!({ "taskId": id, "path": "a.txt", "resolution": "ours" }));
    assert_eq!(cs["files"], json!([{ "path": "b.txt", "kind": "bothModified" }]));
    let cs = h.ok("resolve_conflict_file", json!({ "taskId": id, "path": "b.txt", "resolution": "theirs" }));
    assert_eq!(cs["files"], json!([]));
    assert_eq!(cs["readyToCommit"], true);

    let done = h.ok("complete_base_merge", json!({ "taskId": id, "push": false }));
    assert_eq!(done["mergeInProgress"], false);
    assert_eq!(read(&wt, "a.txt"), "task\n");
    assert_eq!(read(&wt, "b.txt"), "main\n");
    assert_eq!(read(&wt, "c.txt"), "main only\n");
    let parents = h.git(&wt, &["rev-list", "--parents", "-n", "1", "HEAD"]);
    assert_eq!(parents.split_whitespace().count(), 3, "マージコミット: {parents}");
    assert_eq!(parents.split_whitespace().nth(1).unwrap(), task_head.trim());

    // 取り込み済みなら何も起きない
    let again = h.ok("start_base_merge", json!({ "taskId": id }));
    assert_eq!(again["mergeInProgress"], false);

    // 中止すると取り込み前に戻る
    write(&repo, "a.txt", "main 2\n");
    h.commit_all(&repo, "main again");
    let before = h.git(&wt, &["rev-parse", "HEAD"]);
    let cs = h.ok("start_base_merge", json!({ "taskId": id }));
    assert_eq!(cs["files"].as_array().unwrap().len(), 1);
    h.ok("abort_base_merge", json!({ "taskId": id }));
    assert_eq!(h.ok("get_conflict_state", json!({ "taskId": id }))["mergeInProgress"], false);
    assert_eq!(h.git(&wt, &["rev-parse", "HEAD"]), before);
    assert_eq!(read(&wt, "a.txt"), "task\n");

    // 手動（エディタ / AI）で編集した内容を markResolved で採用する
    let cs = h.ok("start_base_merge", json!({ "taskId": id }));
    assert_eq!(cs["files"].as_array().unwrap().len(), 1);
    write(&wt, "a.txt", "task + main 2\n");
    let cs = h.ok("resolve_conflict_file", json!({ "taskId": id, "path": "a.txt", "resolution": "markResolved" }));
    assert_eq!(cs["readyToCommit"], true);
    h.ok("complete_base_merge", json!({ "taskId": id, "push": false }));
    assert_eq!(read(&wt, "a.txt"), "task + main 2\n");
    assert_eq!(h.ok("get_git_status", json!({ "taskId": id }))["files"], json!([]));
}

/// origin がある場合は fetch した origin/<base> を取り込み、完了時に push する。
#[test]
fn base_merge_from_origin_and_push() {
    let h = Harness::new();
    let repo = h.root().join("app");
    h.init_repo(&repo, &[("a.txt", "base\n")]);
    let origin = h.add_local_origin(&repo);
    let app = h.ok("add_project", json!({ "path": repo }));
    let t = create_task(&h, &app, "task/sync", "codex");
    let wt = path_of(&t, "worktreePath");

    write(&wt, "task.txt", "task\n");
    h.commit_all(&wt, "task");
    // 別の人が origin/main を進めた状況（ローカルの main は古いまま）
    let other = h.root().join("other");
    h.git(h.root(), &["clone", "-q", origin.to_str().unwrap(), other.to_str().unwrap()]);
    write(&other, "remote.txt", "remote\n");
    h.commit_all(&other, "remote change");
    h.git(&other, &["push", "-q", "origin", "main"]);

    let cs = h.ok("start_base_merge", json!({ "taskId": t["id"] }));
    assert_eq!(cs["baseRef"], "origin/main");
    assert_eq!(cs["files"], json!([]));
    assert_eq!(cs["readyToCommit"], true);
    assert_eq!(read(&wt, "remote.txt"), "remote\n");
    let done = h.ok("complete_base_merge", json!({ "taskId": t["id"], "push": true }));
    assert_eq!(done["mergeInProgress"], false);
    let pushed = h.git(&origin, &["rev-parse", "refs/heads/task/sync"]);
    assert_eq!(pushed, h.git(&wt, &["rev-parse", "HEAD"]));
    let st = h.ok("get_git_status", json!({ "taskId": t["id"] }));
    assert_eq!(st["upstream"], "origin/task/sync");
    assert_eq!(st["ahead"], 0);
}

/// 偽の gh（呼び出しを記録し、状態をファイルで持つ）を PATH の先頭に置く。
fn install_fake_gh(h: &Harness) -> PathBuf {
    let state = h.root().join("gh-state");
    std::fs::create_dir_all(&state).unwrap();
    let script = r#"#!/bin/sh
S=__STATE__
printf '%s\0' "$@" >> "$S/calls"
printf '\0' >> "$S/calls"
case "$1 $2" in
  "auth status") exit 0 ;;
  "pr view")
    if [ ! -f "$S/state" ]; then
      printf 'no pull requests found for branch "%s"\n' "$3" >&2
      exit 1
    fi
    printf '{"number":7,"url":"https://github.com/example/app/pull/7","title":"%s","state":"%s","isDraft":%s,"headRefName":"%s","baseRefName":"%s","mergeable":"MERGEABLE","mergeStateStatus":"CLEAN","reviewDecision":"","reviews":[],"statusCheckRollup":[{"__typename":"CheckRun","name":"ci","status":"COMPLETED","conclusion":"SUCCESS","detailsUrl":"https://example.invalid/ci"}]}\n' \
      "$(cat "$S/title")" "$(cat "$S/state")" "$(cat "$S/draft")" "$(cat "$S/head")" "$(cat "$S/base")"
    ;;
  "pr create")
    shift 2
    printf false > "$S/draft"
    while [ $# -gt 0 ]; do
      case "$1" in
        --head) printf '%s' "$2" > "$S/head"; shift 2 ;;
        --base) printf '%s' "$2" > "$S/base"; shift 2 ;;
        --title) printf '%s' "$2" > "$S/title"; shift 2 ;;
        --body) shift 2 ;;
        --draft) printf true > "$S/draft"; shift ;;
        *) echo "unknown arg $1" >&2; exit 91 ;;
      esac
    done
    printf OPEN > "$S/state"
    echo "https://github.com/example/app/pull/7"
    ;;
  "pr merge") printf MERGED > "$S/state" ;;
  *) echo "unexpected gh call: $*" >&2; exit 90 ;;
esac
"#
    .replace("__STATE__", &sh_quote(&state.display().to_string()));
    write_script(&h.bin.join("gh"), &script);
    state
}

fn gh_calls(state: &Path) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(state.join("calls")).unwrap_or_default();
    text.split("\0\0")
        .filter(|c| !c.is_empty())
        .map(|c| c.split('\0').map(String::from).collect())
        .collect()
}

#[test]
fn pull_request_commands_build_gh_arguments_and_parse_results() {
    let h = Harness::new();
    let gh_state = install_fake_gh(&h);
    let repo = h.root().join("app");
    h.init_repo(&repo, &[("a.txt", "base\n")]);
    let origin = h.add_local_origin(&repo);
    let app = h.ok("add_project", json!({ "path": repo }));
    let t = create_task(&h, &app, "task/pr", "claude");
    let wt = path_of(&t, "worktreePath");
    let id = t["id"].clone();
    write(&wt, "feature.txt", "feature\n");
    h.commit_all(&wt, "feature");

    // (7) PR が無い
    assert_eq!(h.ok("get_pull_request", json!({ "taskId": id })), Value::Null);
    assert_eq!(gh_calls(&gh_state), vec![vec!["pr", "view", "task/pr", "--json", PR_JSON_FIELDS]]);

    // 入力が不正なら push もしない
    h.err(
        "create_pull_request",
        json!({ "req": { "taskId": id, "title": "  ", "body": "", "base": null, "draft": false } }),
        "invalidInput",
    );
    assert!(h.invoke("create_pull_request", json!({})).is_err());
    assert!(raitei_lib::git::git(&h.state.env, &origin, &["rev-parse", "--verify", "refs/heads/task/pr"]).is_err());
    h.err("merge_pull_request", json!({ "req": { "taskId": id, "method": "squash" } }), "invalidInput");

    // (4) 作成: push -u → gh pr create → 番号で取り直す
    let body = "本文 `code` $HOME $(touch pwned)\n2 行目";
    let pr = h.ok(
        "create_pull_request",
        json!({ "req": { "taskId": id, "title": "PR タイトル", "body": body, "base": null, "draft": true } }),
    );
    assert_eq!(pr["number"], 7);
    assert_eq!(pr["state"], "open");
    assert_eq!(pr["isDraft"], true);
    assert_eq!(pr["headBranch"], "task/pr");
    assert_eq!(pr["baseBranch"], "main");
    assert_eq!(pr["checks"]["passed"], 1);
    assert_eq!(pr["hasConflicts"], false);
    assert_eq!(
        h.git(&origin, &["rev-parse", "refs/heads/task/pr"]),
        h.git(&wt, &["rev-parse", "HEAD"]),
        "PR 作成前に push されている"
    );
    assert_eq!(h.ok("get_git_status", json!({ "taskId": id }))["upstream"], "origin/task/pr");
    assert!(!wt.join("pwned").exists());
    let calls = gh_calls(&gh_state);
    assert_eq!(
        calls[1],
        vec!["pr", "create", "--head", "task/pr", "--base", "main", "--title", "PR タイトル", "--body", body, "--draft"]
    );
    assert_eq!(calls[2], vec!["pr", "view", "7", "--json", PR_JSON_FIELDS]);
    assert_eq!(h.ok("get_task", json!({ "taskId": id }))["prNumber"], 7);

    // (7) 状態確認
    let pr = h.ok("get_pull_request", json!({ "taskId": id }));
    assert_eq!(pr["number"], 7);
    assert_eq!(pr["mergeable"], "mergeable");
    assert_eq!(pr["mergeStateStatus"], "CLEAN");

    // (5) マージ: gh pr merge <n> --squash（--delete-branch は付けない）→ 状態を取り直し → origin のブランチだけ消す
    let merged = h.ok(
        "merge_pull_request",
        json!({ "req": { "taskId": id, "method": "squash", "deleteRemoteBranch": true } }),
    );
    assert_eq!(merged["state"], "merged");
    let calls = gh_calls(&gh_state);
    assert!(calls.contains(&vec!["pr".into(), "merge".into(), "7".into(), "--squash".into()]), "{calls:?}");
    assert!(raitei_lib::git::git(&h.state.env, &origin, &["rev-parse", "--verify", "refs/heads/task/pr"]).is_err());
    assert!(wt.is_dir(), "worktree は残る");
    h.git(&repo, &["rev-parse", "--verify", "refs/heads/task/pr"]);
}

/// stream-json を出す偽の claude。プロンプトに SLOW を含むと停止されるまで待つ。
/// 競合マーカーのある a.txt があれば解消して `git add` する（AI による解消の代わり）。
fn install_fake_claude(h: &Harness) -> PathBuf {
    let state = h.root().join("claude-state");
    std::fs::create_dir_all(&state).unwrap();
    let script = r#"#!/bin/sh
S=__STATE__
printf '%s\n' "$*" >> "$S/args"
prompt=$(cat)
printf '%s\n' "$prompt" > "$S/last_prompt"
echo '{"type":"system","subtype":"hook_started"}'
echo '{"type":"system","subtype":"init","session_id":"fake-session","model":"fake-model"}'
case "$prompt" in
  *SLOW*)
    trap 'exit 143' TERM
    while :; do sleep 0.1; done
    ;;
esac
if [ -f a.txt ] && grep -q '^<<<<<<<' a.txt; then
  echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"a.txt"}}]}}'
  printf 'resolved by agent\n' > a.txt
  git add a.txt
  echo '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok","is_error":false}]}}'
fi
echo '{"type":"assistant","message":{"content":[{"type":"text","text":"完了しました"}]}}'
echo '{"type":"result","subtype":"success","is_error":false,"result":"完了しました","duration_ms":5,"total_cost_usd":0.001}'
"#
    .replace("__STATE__", &sh_quote(&state.display().to_string()));
    write_script(&h.bin.join("claude"), &script);
    state
}

#[test]
fn agent_commands_run_cancel_and_conflict_request_with_fake_cli() {
    let h = Harness::new();
    let claude_state = install_fake_claude(&h);

    let env = h.ok("get_environment", json!({}));
    assert_eq!(PathBuf::from(s(&env["claude"]["path"])), h.bin.join("claude"));
    assert!(env["git"]["path"].is_string());

    let repo = h.root().join("app");
    h.init_repo(&repo, &[("a.txt", "base\n")]);
    let app = h.ok("add_project", json!({ "path": repo }));
    let t = create_task(&h, &app, "task/agent", "claude");
    let wt = path_of(&t, "worktreePath");
    let id = t["id"].clone();

    h.err("send_agent_message", json!({ "req": { "taskId": id, "text": "  " } }), "invalidInput");

    // 1 ターン目 → session_id 保存 → 2 ターン目は --resume
    let run = h.ok("send_agent_message", json!({ "req": { "taskId": id, "text": "こんにちは" } }));
    assert_eq!(run["agent"], "claude");
    let evs = h.sink.wait_finished(s(&run["runId"]), Duration::from_secs(20));
    let types: Vec<_> = evs.iter().map(|e| e.event_type()).collect();
    assert_eq!(types, ["user_message", "session_started", "assistant_text", "result", "run_finished"]);
    assert_eq!(h.ok("get_task", json!({ "taskId": id }))["agentSessionId"], "fake-session");
    let run2 = h.ok("send_agent_message", json!({ "req": { "taskId": id, "text": "続き" } }));
    h.sink.wait_finished(s(&run2["runId"]), Duration::from_secs(20));
    let args = read(&claude_state, "args");
    // get_environment が呼んだ `--version` は除く
    let lines: Vec<_> = args.lines().filter(|l| l.starts_with("-p")).collect();
    assert_eq!(lines[0], "-p --output-format stream-json --verbose --permission-mode acceptEdits");
    assert_eq!(lines[1], "-p --output-format stream-json --verbose --permission-mode acceptEdits --resume fake-session");

    // 実行中は二重送信を拒否し、停止できる
    let slow = h.ok("send_agent_message", json!({ "req": { "taskId": id, "text": "SLOW" } }));
    let state = h.ok("get_agent_run_state", json!({ "taskId": id }));
    assert_eq!(state["running"], true);
    assert_eq!(state["runId"], slow["runId"]);
    h.err("send_agent_message", json!({ "req": { "taskId": id, "text": "割り込み" } }), "agent");
    h.err("reset_agent_session", json!({ "taskId": id }), "agent");
    h.ok("cancel_agent_run", json!({ "taskId": id }));
    let evs = h.sink.wait_finished(s(&slow["runId"]), Duration::from_secs(20));
    let finished = serde_json::to_value(&evs.last().unwrap().event).unwrap();
    assert_eq!(finished["cancelled"], true, "{finished}");
    assert_eq!(h.ok("get_agent_run_state", json!({ "taskId": id }))["running"], false);

    // コンフリクトが無いときは AI に依頼できない
    h.err("request_agent_conflict_resolution", json!({ "taskId": id }), "invalidInput");

    // コンフリクトを作り、エージェントに解消を依頼する
    write(&wt, "a.txt", "task\n");
    h.commit_all(&wt, "task");
    write(&repo, "a.txt", "main\n");
    h.commit_all(&repo, "main");
    let cs = h.ok("start_base_merge", json!({ "taskId": id }));
    assert_eq!(cs["files"].as_array().unwrap().len(), 1);
    let run = h.ok("request_agent_conflict_resolution", json!({ "taskId": id }));
    h.sink.wait_finished(s(&run["runId"]), Duration::from_secs(20));
    let prompt = read(&claude_state, "last_prompt");
    assert!(prompt.contains("- a.txt") && prompt.contains("main"), "{prompt}");
    let cs = h.ok("get_conflict_state", json!({ "taskId": id }));
    assert_eq!(cs["files"], json!([]));
    assert_eq!(cs["readyToCommit"], true);
    h.ok("complete_base_merge", json!({ "taskId": id, "push": false }));
    assert_eq!(read(&wt, "a.txt"), "resolved by agent\n");

    // リセットで履歴と session_id を消す
    h.ok("reset_agent_session", json!({ "taskId": id }));
    assert_eq!(h.ok("get_agent_history", json!({ "taskId": id, "afterSeq": null })), json!([]));
    assert_eq!(h.ok("get_task", json!({ "taskId": id }))["agentSessionId"], Value::Null);
}
