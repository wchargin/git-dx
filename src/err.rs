#[derive(Debug)]
pub enum Error {
    /// A user-provided commit reference does not exist.
    NoSuchCommit(String),
    /// A commit message is expected to have a trailer with the given key, but does not.
    MissingTrailer { oid: String, key: String },
    /// A commit message is expected to have at most one trailer with the given key, but has
    /// more than one.
    DuplicateTrailer { oid: String, key: String },
    /// User-supplied text (e.g., a commit message) was improperly encoded. Data not in UTF-8 must
    /// be declared as such via the `i18n.commitEncoding` setting at commit time. For details, see
    /// `man git-commit`.
    InvalidEncoding {
        oid: String,
        err: std::string::FromUtf8Error,
    },
    /// The `git(1)` binary behaved unexpectedly: e.g., `rev-parse --verify REVISION` returned
    /// success but did not write an object ID to standard output.
    GitContract(String),
    /// Underlying IO error (e.g., failure to invoke `git`).
    IoError(std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Parse user-supplied bytes that are expected to represent valid UTF-8, failing with an
    /// `InvalidEncoding` error referring to `oid` if the bytes are not valid UTF-8.
    pub fn require_utf8(buffer: Vec<u8>, oid: &str) -> Result<String> {
        String::from_utf8(buffer).map_err(|e| Error::InvalidEncoding {
            oid: oid.to_string(),
            err: e,
        })
    }
}

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
