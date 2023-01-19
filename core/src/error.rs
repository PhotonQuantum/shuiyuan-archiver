use std::error::Error as StdError;

// TODO thiserror
pub type BoxedError = Box<dyn StdError + Send + Sync>;
pub type Result<T, E = BoxedError> = std::result::Result<T, E>;
