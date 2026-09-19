# raitei

AI コーディング統合環境（Tauri v2 + React + Rust）。プロジェクトごとに複数の git worktree をタスクとして並列に扱い、ローカルの `claude` / `codex` CLI をヘッドレスで接続して開発し、PR の作成・状態確認・コンフリクト解消・マージまでを 1 つの UI で行う。

設計は [docs/design.md](docs/design.md) を参照。

## 開発

```sh
pnpm install
pnpm tauri dev      # 起動
pnpm tauri build    # .app / .dmg を生成
cd src-tauri && cargo test
```

前提: macOS (Apple Silicon)、Rust stable、Node.js、pnpm、`git`、`gh`（ログイン済み）、`claude` / `codex` CLI。
