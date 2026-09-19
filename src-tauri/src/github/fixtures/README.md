# WS-D の PR JSON テスト

`gh_pr_view_*.json` は公開形式に合わせた架空のサンプルです。
実リポジトリから採取したデータではありません。
担当範囲を守るため、設計書 §4.6 の `tests/fixtures/` ではなく `src/github/fixtures/` に配置しています。

- `gh_pr_view_mixed.json`: CheckRun と StatusContext の混在、レビュー、削除済みユーザー、競合。
- `gh_pr_view_empty.json`: Draft、レビューなし、CI なし、判定待ち。
- その他の状態・結論・不正 JSON は `parse.rs` のテストでサンプルを変換して検証。
- CLI 呼び出しは `test_support.rs` の一時ディレクトリ内の偽 gh/git のみを使用。インストール済み CLI による作成・マージ・push は行わない。

参照: [gh pr view](https://cli.github.com/manual/gh_pr_view)、[gh pr create](https://cli.github.com/manual/gh_pr_create)、[gh pr merge](https://cli.github.com/manual/gh_pr_merge)。

## 統合時の申し送り

型・既存の公開関数シグネチャ・command・イベントは変更していません。
PR 作成時の push は WS-C の `repo::push(env, worktree, branch, true)` に委譲します。
リモート削除は既存の `git::git` 経由で `git push origin --delete <branch>` を呼び、ローカル worktree は操作しません。

WS-H の CI 集計表示では、`neutral` と `skipped` を `passed`、`cancelled` を `failed`、`unknown` を `pending` に含めます。
各チェックの元の分類は `checks[].status` に保持します。
空の `reviewDecision` は `None`、削除済みレビュー投稿者は `ghost` として扱います。

`gh pr merge` の成功は merge queue への登録だけの場合があります。
再取得で `MERGED` を確認できた場合だけリモートブランチを削除します。
キュー待ちでは取得した `OPEN` を返し、後日の自動削除予約は保存しません。
この場合はマージ完了後に削除を伴うマージ操作を再実行するか、リモートブランチを別途管理します。
削除に失敗した場合は、マージ済みであることを明記した `AppError::Git` を返します。

WS-0 で未確認: WS-A/B/C/H 統合後の Tauri 実機操作、実 GitHub に対する作成・マージ・削除、実出力 fixture の採取。
本作業では実 GitHub への書き込みは禁止されているため、サンプルと偽 CLI で検証しています。
