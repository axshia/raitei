# raitei 設計メモ（MVP）

この文書は、raitei を複数の実装者（Fleet の各エージェント）が並列に実装するための参照用設計書です。読み終えると、自分の担当ファイル、守るべき契約（型・command・イベント）、他担当との依存関係が分かります。

対象外: ビルド配布（署名・公証）、Windows / Linux 対応、PTY ターミナル埋め込み。

## 1. 前提

- Tauri v2 + React 19 + Vite + TypeScript、状態管理は zustand、パッケージマネージャは pnpm
- 対象は macOS Apple Silicon。OS 依存箇所は `shell_env.rs` に閉じ込め、将来のクロスプラットフォーム化に備える
- git 操作は `git` CLI、GitHub 操作は `gh` CLI（ログイン済み前提）を Rust から呼ぶ
- エージェントはローカルの `claude` / `codex` CLI をヘッドレスで呼ぶ。PTY / xterm.js は使わない
- 永続化は SQLite（`rusqlite` bundled）。DB は `<app_data_dir>/raitei.db`（macOS では `~/Library/Application Support/jp.ubros.raitei/raitei.db`）

## 2. 用語

| 用語 | 意味 |
|------|------|
| プロジェクト | raitei に登録したローカル git リポジトリ 1 つ |
| タスク | worktree + ブランチ + エージェントセッションの組。UI ではタブ 1 枚 |
| run | エージェントへの 1 メッセージ送信に対応する子プロセス 1 回（1 ターン） |
| 正規化イベント | claude / codex の JSONL 出力を共通型 `AgentEvent` に変換したもの |
| WS-x | 並列実装のワークストリーム（担当単位）。第 8 章を参照 |

## 3. 全体構成

フロントとバックエンドの関係（上から下へ呼び出す）:

```
React UI (features/*)
   |  zustand store (store/*)
   |  invoke ラッパー (api/*)            ^ agent://event
   v                                     |
Tauri commands (commands/*.rs) ----------+
   |          |           |          |
   v          v           v          v
 store/     git/       github/     agent/manager ---> claude / codex 子プロセス
 (SQLite)  (git CLI)   (gh CLI)     agent/{claude,codex} (コマンド組立・JSONL パース)
   \__________\___________\__________/
                shell_env (PATH 解決・外部コマンド実行)
```

command 層は薄いオーケストレーションだけを持ち、ロジックは `git` / `github` / `agent` / `store` に置きます。

## 4. Rust モジュールと契約

### 4.1 ファイル一覧

| パス | 内容 | 担当 | 契約 |
|------|------|------|------|
| `src/lib.rs` | モジュール宣言・Tauri Builder・command 登録 | 統合（WS-0） | command 追加時のみ編集 |
| `src/error.rs` | `AppError` / `AppResult` | 凍結 | 変更は全員合意 |
| `src/models.rs` | Project / Task / リクエスト型 / EnvironmentInfo | 凍結 | 同上 |
| `src/state.rs` | `AppState` / `TauriSink` / `blocking()` | 凍結 | 同上 |
| `src/shell_env.rs` | ログインシェル PATH 解決・`run()` | WS-A | シグネチャ凍結 |
| `src/store/mod.rs`（+ 任意の下位ファイル） | SQLite 永続化 | WS-B | 公開メソッド凍結 |
| `src/git/types.rs` | git 系 IPC 型 | 凍結 | |
| `src/git/{mod,repo,worktree,status,conflict}.rs` | git CLI ラッパー | WS-C | 公開関数のシグネチャ凍結 |
| `src/github/types.rs` | PR 系 IPC 型 | 凍結 | |
| `src/github/{mod,gh,parse}.rs` | gh CLI ラッパー | WS-D | 公開関数のシグネチャ凍結 |
| `src/agent/types.rs` | `AgentEvent` ほか | 凍結 | |
| `src/agent/runner.rs` | `AgentRunner` trait | 凍結 | |
| `src/agent/{claude,codex,manager}.rs` | ランナー実装・プロセス管理 | WS-E | 公開 API 凍結 |
| `src/commands/system.rs` | 環境情報 | WS-A | |
| `src/commands/{project,task}.rs` | プロジェクト・タスク | WS-B | |
| `src/commands/git.rs` | worktree / status / コンフリクト | WS-C | |
| `src/commands/pr.rs` | PR | WS-D | |
| `src/commands/agent.rs` | エージェント | WS-E | |
| `tests/fixtures/*.jsonl` | 実機で採取した CLI 出力 | WS-E | 追加自由 |

「凍結」のファイルを変える必要が出た場合は、Rust（`models.rs` など）と TS（`src/api/types.ts`）を同じコミットで更新し、他担当に知らせます。

### 4.2 共通規約

- すべての command は `async fn` で `AppResult<T>` を返す。外部コマンドを伴う処理は `state::blocking(move || ...)` で包み、メインスレッドを塞がない
- 外部コマンドは必ず `shell_env::ShellEnv` 経由で起動する（`std::process::Command::new` を直接使わない）
- 出力パースは副作用のない `parse_*` 関数に分け、ユニットテストを書く。git の結合テストは `tempfile` で一時リポジトリを作って行う
- シリアライズは struct が camelCase、`AgentEvent` の中身だけ snake_case（`#[serde(tag = "type", rename_all = "snake_case")]`）
- エラーは `{ kind, message }` の JSON でフロントに渡る。`kind` は `notImplemented | notFound | invalidInput | git | gh | agent | command | db | io`
- 仮実装は `AppError::NotImplemented("モジュール::関数")` を返す。`todo!()` / `unimplemented!()` は使わない

### 4.3 shell_env（WS-A）

GUI から起動した macOS アプリは PATH が `/usr/bin:/bin:/usr/sbin:/sbin` 程度しかなく、`claude` や `gh` が見つかりません。そこで起動時に 1 回だけ `$SHELL -ilc` で PATH を取得してキャッシュします。

- `ShellEnv::resolve()`: `$SHELL`（なければ `/bin/zsh`）を `-ilc 'printf "__RAITEI__%s__RAITEI__" "$PATH"'` で実行し、マーカー間を取り出す。rc ファイルの出力混入に備えてマーカーを使う。タイムアウト 5 秒。失敗時は現行 PATH + `/opt/homebrew/bin` などで代替
- `which(cmd)`, `command(program)`, `tokio_command(program)`, `run(env, program, args, cwd)`

### 4.4 store（WS-B）

テーブル定義（案。列の追加は自由、公開メソッドは変えない）:

```sql
CREATE TABLE projects (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, repo_path TEXT NOT NULL UNIQUE,
  default_branch TEXT NOT NULL, created_at TEXT NOT NULL);
CREATE TABLE tasks (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  title TEXT NOT NULL, branch TEXT NOT NULL, base_branch TEXT NOT NULL,
  worktree_path TEXT NOT NULL, agent TEXT NOT NULL, permission TEXT NOT NULL,
  agent_session_id TEXT, pr_number INTEGER, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
CREATE TABLE agent_events (
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL, run_id TEXT NOT NULL, agent TEXT NOT NULL,
  timestamp TEXT NOT NULL, event_json TEXT NOT NULL,
  PRIMARY KEY (task_id, seq));
```

- マイグレーションは `PRAGMA user_version` で管理
- `Connection` は `Mutex<Connection>` で保持（書き込み頻度が低いため十分）
- 公開メソッド: `open` / `open_in_memory` / `list_projects` / `get_project` / `insert_project` / `delete_project` / `list_tasks` / `get_task` / `insert_task` / `update_task` / `delete_task` / `set_task_agent_session` / `set_task_pr_number` / `append_agent_event`（seq 採番） / `list_agent_events` / `clear_agent_events`
- 現在はメモリ上の仮実装。アプリを再起動するとデータが消える

### 4.5 git（WS-C）

| 関数 | 実行する git |
|------|--------------|
| `repo::repo_root` | `rev-parse --show-toplevel` |
| `repo::init_repo` | `init -b main` → `commit --allow-empty -m "Initial commit"` |
| `repo::default_branch` | `symbolic-ref refs/remotes/origin/HEAD` → main / master の存在確認 → 現在ブランチ |
| `repo::has_origin` / `fetch` / `push` | `remote get-url origin` / `fetch origin <b>` / `push [-u] origin <b>` |
| `worktree::worktree_path_for` | 配置規約 `<repo の親>/<repo 名>.worktrees/<branch の / を - に>` |
| `worktree::list_worktrees` | `worktree list --porcelain` |
| `worktree::add_worktree` | `worktree add -b <branch> <path> <base>`（既存ブランチなら `-b` なし） |
| `worktree::remove_worktree` | `worktree remove [--force]` → `worktree prune` |
| `worktree::delete_branch` | `branch -d / -D` |
| `status::status` | `status --porcelain=v2 --branch` + MERGE_HEAD 確認 |
| `conflict::*` | 下記フロー |

コンフリクト解消フロー（タスクの worktree 上で実行）:

```
start_base_merge ─> 競合なし ─────────────────────────────> complete_base_merge(push)
      │                                                          ^
      └─> 競合あり ─> ファイルごとに resolve_conflict_file ─────┤
                      (ours / theirs / markResolved)             │
                  └─> request_agent_conflict_resolution ─> AI が編集・add ─┘
      いつでも: abort_base_merge
```

- `merge_base_into`: origin があれば `fetch origin <base>` → `merge --no-ff --no-commit origin/<base>`、なければローカル `<base>` を merge
- 未解決ファイルは `status --porcelain=v1` の `UU/AA/DU/UD/AU/UA/DD` を `parse_unmerged` で抽出
- `read_conflict_file`: 作業ツリー + `git show :2:<path>`（ours）+ `:3:<path>`（theirs）
- `commit_merge`: 未解決が残っていれば `InvalidInput`、なければ `commit --no-edit`
- AI 依頼のプロンプトは `conflict::build_agent_prompt` が作る（コミットはさせず add まで）

### 4.6 github（WS-D）

- PR 取得: `gh pr view <branch> --json <PR_JSON_FIELDS>` を worktree で実行し、`parse::parse_pr_view` で `PullRequestStatus` に変換。「no pull requests found」は `None`
- 作成: command 層で `git push -u origin <branch>` → `gh pr create --head --base --title --body [--draft]` → 出力 URL から番号を得て `pr_view`
- マージ: `gh pr merge <n> --merge|--squash|--rebase`。`--delete-branch` はローカルブランチを操作し worktree と衝突するため使わない。リモートブランチ削除は `git push origin --delete <branch>`
- `has_conflicts` = `mergeable == CONFLICTING` または `mergeStateStatus == DIRTY`
- CI チェック集計: `statusCheckRollup` の CheckRun（`status` + `conclusion`）と StatusContext（`state`）を pending / success / failure / neutral / skipped / cancelled / unknown に正規化
- テストは `gh` の実出力を fixture（`tests/fixtures/gh_pr_view_*.json`）にして `parse_pr_view` を検証

### 4.7 agent（WS-E）

実行モデルは「1 メッセージ = 1 子プロセス」です。会話の継続は各 CLI の resume で行い、raitei はセッション ID を Task に保存します。

| | claude | codex |
|--|--------|-------|
| 新規 | `claude -p --output-format stream-json --verbose --permission-mode <m>`（stdin にプロンプト） | `codex exec --json --skip-git-repo-check <sandbox> -`（stdin） |
| 継続 | 上記 + `--resume <session_id>` | `codex exec resume --json ... <thread_id> -` |
| safe | `--permission-mode acceptEdits` | `-c sandbox_mode="workspace-write"` |
| full | `--permission-mode bypassPermissions` | `--dangerously-bypass-approvals-and-sandbox` |
| セッション ID | `system/init` の `session_id` | `thread.started` の `thread_id` |

2026-09-19 に実機（claude 2.1.277 / codex-cli 0.155.0）で採取した出力を `src-tauri/tests/fixtures/` に置いています。イベント対応は `agent/claude.rs` / `agent/codex.rs` の冒頭コメントにまとめました。要点:

- claude はフックや `rate_limit_event`、`thinking_tokens` などのノイズ行が多い。`system/init` 以外の system 行は無視する
- claude の `assistant` 行は content ブロック単位で届く（text / thinking / tool_use が別行）。`--include-partial-messages` を付ければトークン単位の差分も取れるが、MVP では使わない
- codex は `item.started` / `item.completed` の対で届く。`command_execution` の開始を ToolUse、完了を ToolResult に対応させる
- codex は設定によって `item.completed` の `type:"error"`（警告）を出す。致命的ではないので `Error` イベントとして表示するだけにする
- codex の stderr には MCP 接続エラーなどが大量に出る。`Stderr` イベントとして流すが、UI では既定で折りたたむ

`AgentRunner` trait（凍結）:

```rust
pub trait AgentRunner: Send + Sync {
    fn kind(&self) -> AgentKind;
    fn build_command(&self, spec: &RunSpec) -> CommandSpec;               // 純粋関数
    fn parse_line(&self, line: &str, state: &mut ParseState) -> Vec<AgentEvent>; // 純粋関数
}
```

`AgentManager`（WS-E が実装）の責務:

1. タスクごとに同時 1 run。実行中の送信は `AppError::Agent`
2. `UserMessage` を記録してから `runner.build_command` のコマンドを `tokio_command` で起動（cwd = worktree）
3. stdout を行ごとに `parse_line` → `store.append_agent_event`（seq 採番・永続化）→ `sink.emit`
4. `SessionStarted` を受けたら `store.set_task_agent_session` で保存
5. stderr は `Stderr` イベントに変換
6. 終了時（正常・異常・キャンセル）に必ず `RunFinished` を 1 回送る
7. `cancel` で子プロセスを kill（`RunFinished { cancelled: true }`）

将来の拡張: `claude --input-format stream-json` による常駐プロセス化（割り込み送信・ツール承認 UI）は、`AgentRunner` とは別の `SessionRunner` trait を追加して `AgentManager` から切り替える想定です。MVP ではやりません。

## 5. Tauri command 一覧

JS からの引数名は camelCase（Tauri が Rust の snake_case に変換）。ラッパーは `src/api/*.ts` にあります。

| command | 引数（JS） | 戻り値 | 担当 |
|---------|-----------|--------|------|
| `get_environment` | なし | `EnvironmentInfo` | A |
| `list_projects` | なし | `Project[]` | B |
| `add_project` | `path` | `Project` | B |
| `create_project` | `req: CreateProjectRequest` | `Project` | B |
| `remove_project` | `projectId` | `void` | B |
| `list_tasks` | `projectId` | `Task[]` | B |
| `get_task` | `taskId` | `Task` | B |
| `create_task` | `req: CreateTaskRequest` | `Task` | B |
| `update_task` | `req: UpdateTaskRequest` | `Task` | B |
| `delete_task` | `taskId, options: DeleteTaskOptions` | `void` | B |
| `list_worktrees` | `projectId` | `WorktreeInfo[]` | C |
| `get_git_status` | `taskId` | `GitStatus` | C |
| `start_base_merge` | `taskId` | `ConflictState` | C |
| `get_conflict_state` | `taskId` | `ConflictState` | C |
| `read_conflict_file` | `taskId, path` | `ConflictFileContent` | C |
| `resolve_conflict_file` | `taskId, path, resolution` | `ConflictState` | C |
| `abort_base_merge` | `taskId` | `void` | C |
| `complete_base_merge` | `taskId, push` | `ConflictState` | C |
| `request_agent_conflict_resolution` | `taskId` | `AgentRunInfo` | C |
| `get_pull_request` | `taskId` | `PullRequestStatus \| null` | D |
| `create_pull_request` | `req: CreatePrRequest` | `PullRequestStatus` | D |
| `merge_pull_request` | `req: MergePrRequest` | `PullRequestStatus` | D |
| `send_agent_message` | `req: SendMessageRequest` | `AgentRunInfo` | E |
| `cancel_agent_run` | `taskId` | `void` | E |
| `get_agent_history` | `taskId, afterSeq?` | `AgentEventEnvelope[]` | E |
| `get_agent_run_state` | `taskId` | `AgentRunState` | E |
| `reset_agent_session` | `taskId` | `void` | E |

## 6. イベント（Rust → フロント）

| イベント名 | ペイロード | 発生源 |
|-----------|-----------|--------|
| `agent://event` | `AgentEventEnvelope` | `AgentManager`（`TauriSink` 経由） |

```ts
interface AgentEventEnvelope {
  taskId: string; runId: string; agent: "claude" | "codex";
  seq: number;        // タスク内で単調増加。重複排除キー
  timestamp: string;
  event: AgentEvent;  // type: user_message | session_started | assistant_text | thinking
                      //       | tool_use | tool_result | result | error | stderr | run_finished
}
```

フロントは起動時に 1 回だけ購読し（`initAgentEventBridge`）、タブを開いたときに `get_agent_history` で過去分を取得します。両者の重複は `seq` で除きます。PR 状態は push 型にせず、フロントがポーリングします（タブ表示中 30 秒間隔を想定）。

## 7. フロントエンド構成

```
src/
  api/            契約（凍結）: types.ts と invoke ラッパー
  store/
    projectStore.ts  プロジェクト・タスク一覧（WS-F）
    tabStore.ts      開いているタブ・右パネル選択（WS-F）
    agentStore.ts    会話イベント・実行中フラグ（WS-G）
    prStore.ts       PR 状態・コンフリクト状態（WS-H）
  styles/global.css  デザイントークン・レイアウト（WS-F）
  App.tsx            シェル（WS-F）
  components/layout/ Sidebar, TaskTabs（WS-F）
  features/
    projects/     AddProjectButtons, ProjectSection（WS-F）
    tasks/        TaskList, NewTaskForm, TaskView（WS-F）
    worktrees/    WorktreePanel（WS-F）
    chat/         ChatPanel ほか（WS-G）
    pr/           PrPanel ほか（WS-H）
    conflicts/    ConflictPanel ほか（WS-H）
```

画面構成:

```
+------------+-----------------------------------------------+
| raitei     | [● task A] [task B] [task C]           タブ    |
| [追加]     +------------------------------+----------------+
| project 1  |  チャット（ChatPanel）         | [PR][衝突][WT] |
|   task A   |   user / assistant / tool     |  PrPanel       |
|   task B   |   ...                         |  ConflictPanel |
|   + 新規   |  [入力欄           ][送信]    |  WorktreePanel |
| project 2  |                              |                |
+------------+------------------------------+----------------+
```

- 非アクティブなタブもアンマウントせず `display: none` で保持する（入力中テキストやスクロール位置を失わないため）
- 各パネルは `taskId` だけを props で受け、状態は自分のストアから読む。パネル同士で props を渡し合わない
- ストア間の連携は `getState()` 経由で行う（例: PR 作成後に `useProjectStore.getState().upsertTask()`）
- CSS は `global.css` のトークン（`--bg` など）を使い、feature 固有のスタイルは各 feature ディレクトリ内の CSS ファイルに置く

## 8. 並列実装の分担

| WS | 範囲 | 主な依存 |
|----|------|----------|
| A | `shell_env.rs`, `commands/system.rs` | なし |
| B | `store/`, `commands/project.rs`, `commands/task.rs` | C の `repo_root` / `init_repo` / `add_worktree` / `remove_worktree` / `delete_branch`（シグネチャのみ） |
| C | `git/*`（types 以外）, `commands/git.rs` | なし（`request_agent_conflict_resolution` は E の `start_run` を呼ぶだけ） |
| D | `github/*`（types 以外）, `commands/pr.rs` | C の `repo::push` |
| E | `agent/{claude,codex,manager}.rs`, `commands/agent.rs`, fixture | B の `append_agent_event` / `set_task_agent_session` |
| F | `App.tsx`, `styles/`, `components/layout/`, `features/{projects,tasks,worktrees}/`, `store/{projectStore,tabStore}.ts` | なし |
| G | `features/chat/`, `store/agentStore.ts` | なし（イベント型は凍結済み） |
| H | `features/{pr,conflicts}/`, `store/prStore.ts` | なし |

すべての WS は契約のシグネチャに依存するだけなので、同時に着手できます。結合時の確認（WS-0）は、全 WS のマージ後に `cargo test`、`pnpm build`、`pnpm tauri build` と実機での一連操作を行います。

## 9. 未決事項

- 未確認: `codex exec resume` が `--json` と `-c sandbox_mode` を受け付けるか。help には `-c` がある。`--json` は WS-E が実機で確認する
- 未確認: claude の `--permission-mode acceptEdits` で Bash ツールが拒否されたときの挙動。結果の `permission_denials` を UI に出すかは WS-E / G で判断する
- 未実施: 長時間 run の出力量制限（`agent_events` 肥大化対策）。MVP では制限しない
- 未実施: worktree 配置先の設定 UI。MVP は第 4.5 節の規約に固定
