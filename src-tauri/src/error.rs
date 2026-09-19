//! アプリ共通エラー型（契約: 凍結）。
//!
//! フロントへは `{ "kind": "...", "message": "..." }` の JSON としてシリアライズされる。
//! TS 側の対応型は `src/api/types.ts` の `AppError`。

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("未実装です: {0}")]
    NotImplemented(&'static str),
    #[error("見つかりません: {0}")]
    NotFound(String),
    #[error("入力が不正です: {0}")]
    InvalidInput(String),
    #[error("git エラー: {0}")]
    Git(String),
    #[error("gh エラー: {0}")]
    Gh(String),
    #[error("エージェントエラー: {0}")]
    Agent(String),
    #[error("外部コマンドエラー: {0}")]
    Command(String),
    #[error("DB エラー: {0}")]
    Db(String),
    #[error("I/O エラー: {0}")]
    Io(#[from] std::io::Error),
}

impl AppError {
    /// TS 側 `AppErrorKind` と一致させること。
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::NotImplemented(_) => "notImplemented",
            AppError::NotFound(_) => "notFound",
            AppError::InvalidInput(_) => "invalidInput",
            AppError::Git(_) => "git",
            AppError::Gh(_) => "gh",
            AppError::Agent(_) => "agent",
            AppError::Command(_) => "command",
            AppError::Db(_) => "db",
            AppError::Io(_) => "io",
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Db(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::InvalidInput(e.to_string())
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("kind", self.kind())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
