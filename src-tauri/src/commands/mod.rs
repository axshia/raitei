//! Tauri command 群。ファイルごとに担当ワークストリームが分かれる。
//!
//! 規約:
//! - すべて `async fn` で `AppResult<T>` を返す
//! - 外部コマンドを伴う処理は `state::blocking` で包む
//! - 引数名は Rust では snake_case、JS からは camelCase で渡る（Tauri の既定変換）
//! - 新しい command を追加したら `lib.rs` の `generate_handler!` と `src/api/` の両方に追加する
//!
//! | ファイル       | 担当 |
//! |----------------|------|
//! | system.rs      | WS-A |
//! | project.rs     | WS-B |
//! | task.rs        | WS-B |
//! | git.rs         | WS-C |
//! | pr.rs          | WS-D |
//! | agent.rs       | WS-E |

pub mod agent;
pub mod git;
pub mod pr;
pub mod project;
pub mod system;
pub mod task;
