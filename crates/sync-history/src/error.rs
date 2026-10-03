/// Why an append, or another operation on the history, stopped.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A git command that failed.
    #[error("git {args}: {message}")]
    Git { args: String, message: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A manifest or a chunk.
    #[error(transparent)]
    Source(#[from] sync_source::Error),
    /// A WAD of the build that could not be read.
    #[error("{path}")]
    Wad { path: String, source: sync_source::Error },
    /// The tip holds something the format does not.
    #[error("the tip: {0}")]
    Tip(String),
    /// A build that cannot be appended as it is.
    #[error("{0}")]
    Build(String),
}
