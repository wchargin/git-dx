use std::process::Command;

mod err {
    #[derive(Debug)]
    pub enum Error {
        /// A user-provided commit reference does not exist.
        NoSuchCommit(String),
        /// The `git(1)` binary behaved unexpectedly: e.g., `rev-parse --verify REVISION` returned
        /// success but did not write an object ID to standard output.
        GitContract(String),
        /// Underlying IO error (e.g., failure to invoke `git`).
        IoError(std::io::Error),
    }

    impl From<std::io::Error> for Error {
        fn from(e: std::io::Error) -> Error {
            Error::IoError(e)
        }
    }

    pub type Result<T> = std::result::Result<T, Error>;
}

fn main() -> err::Result<()> {
    let out = Command::new("git")
        .args(&["rev-parse", "--verify", "HEAD^{commit}"])
        .output()?;
    if !out.status.success() {
        return Err(err::Error::NoSuchCommit(format!(
            "failed to parse commit: {:?}",
            &out.stderr
        )));
    }
    let oid = parse_oid(out.stdout).map_err(|buf| {
        err::Error::GitContract(format!(
            "rev-parse returned success but stdout was: {:?}",
            String::from_utf8_lossy(&buf)
        ))
    })?;
    println!("Head is at {:?}", oid);
    println!("Commit message:\n{:?}", commit_message(&oid)?);
    Ok(())
}

fn commit_message(oid: &str) -> err::Result<String> {
    let out = Command::new("git")
        .args(&["show", "--format=%B", "--no-patch", oid])
        .output()?;
    if !out.status.success() {
        return Err(err::Error::NoSuchCommit(oid.to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn parse_oid(stdout: Vec<u8>) -> Result<String, Vec<u8>> {
    let mut raw = String::from_utf8(stdout).map_err(|e| e.into_bytes())?;
    match raw.pop() {
        Some('\n') => return Ok(raw),
        Some(other) => raw.push(other),
        None => (),
    }
    Err(raw.into_bytes())
}
