#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeamError {
    #[error("login_pending")]
    LoginPending,
    #[error("unauthorized")]
    Unauthorized,
    #[error("not_found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    #[error("conflict")]
    Conflict,
    #[error("directory_unavailable")]
    DirectoryUnavailable,
    #[error("invalid_input")]
    InvalidInput,
    #[error("team_configuration_invalid")]
    ConfigInvalid,
    #[error("team_unavailable")]
    Unavailable,
    #[error("team_storage_unavailable")]
    Storage,
}

impl From<rusqlite::Error> for TeamError {
    fn from(_: rusqlite::Error) -> Self {
        // Raw SQL/dependency messages never become public authentication errors.
        Self::Storage
    }
}
