#[derive(Debug)]
pub enum Error {
    /// A user-provided commit reference does not exist.
    NoSuchCommit(String),
    /// A commit message is expected to have a trailer with the given key, but does not.
    MissingTrailer { oid: String, key: String },
    /// A commit message is expected to have at most one trailer with the given key, but has
    /// more than one.
    DuplicateTrailer { oid: String, key: String },
    /// The `git(1)` binary behaved unexpectedly: e.g., `rev-parse --verify REVISION` returned
    /// success but did not write an object ID to standard output.
    GitContract(String),
    /// Underlying IO error (e.g., failure to invoke `git`).
    IoError(std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::IoError(e)
    }
}

pub fn from_git<F: FnOnce() -> String>(output: &std::process::Output, fmt: F) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let msg = if stderr.is_empty() {
        fmt()
    } else {
        format!("{}: {}", fmt(), stderr)
    };
    Err(Error::GitContract(msg))
}
