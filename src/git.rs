use std::path::PathBuf;
use std::process::Command;

use crate::err;

pub struct GitStore {
    directory: PathBuf,
}

impl GitStore {
    /// Construct a store of the Git repository at the given location. The location is assumed to
    /// be valid for the lifetime of this store: in particular, if the location is given as a
    /// relative path, then the current directory should not be changed.
    pub fn new(repo: PathBuf) -> GitStore {
        GitStore { directory: repo }
    }

    fn git(&self) -> Command {
        let mut cmd = Command::new("git");
        cmd.arg("-C");
        cmd.arg(self.directory.as_os_str());
        cmd.args(&["-c", "i18n.logOutputEncoding=utf-8"]);
        cmd
    }

    pub fn rev_parse(&self, rev: &str) -> err::Result<Option<String>> {
        let out = self.git().args(&["rev-parse", "--verify", rev]).output()?;
        if !out.status.success() {
            return Ok(None);
        };
        parse_oid(out.stdout).map(Some).map_err(|buf| {
            err::Error::GitContract(format!(
                "rev-parse returned success but stdout was: {:?}",
                String::from_utf8_lossy(&buf)
            ))
        })
    }

    pub fn rev_parse_commit(&self, rev: &str) -> err::Result<Option<String>> {
        match self.rev_parse(rev)? {
            None => Ok(None),
            Some(hash) => self.rev_parse(&format!("{}^{{commit}}", hash)),
        }
    }

    pub fn rev_parse_commit_ok(&self, rev: &str) -> err::Result<String> {
        self.rev_parse_commit(rev)?
            .ok_or_else(|| err::Error::NoSuchCommit(rev.to_string()))
    }
}

pub fn parse_oid(stdout: Vec<u8>) -> Result<String, Vec<u8>> {
    let mut raw = String::from_utf8(stdout).map_err(|e| e.into_bytes())?;
    match raw.pop() {
        Some('\n') => return Ok(raw),
        Some(other) => raw.push(other),
        None => (),
    }
    Err(raw.into_bytes())
}
