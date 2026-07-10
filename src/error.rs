use alloc::string::String;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid rules JSON")]
    InvalidJson,
    #[error("invalid regex {pattern:?} in provider {provider:?}: {message}")]
    InvalidRegex {
        provider: String,
        /// As compiled: parameter-name rules are anchored to `^rule$`.
        pattern: String,
        message: String,
    },
}
