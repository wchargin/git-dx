use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use crate::err;

pub struct GitStore {
    directory: PathBuf,
    commits: HashMap<String, Commit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// The canonical object ID for this commit.
    pub oid: String,
    /// The object IDs of this commit's parents, in order.
    pub parents: Vec<String>,
    /// The object ID of this commit's tree.
    pub tree: String,
    /// The raw commit message.
    pub message: String,
}

enum ReadCommit {
    /// The desired commit has the specified full object ID and already exists in the cache. We
    /// only return its key rather than a reference to the object (via a `Cow`) because the
    /// latter would hit an incompleteness in the borrow checker that has not yet been fixed by
    /// Polonius.
    Cached(String),
    /// The desired commit does not exist in cache and has just been read.
    Read(Commit),
}

impl GitStore {
    /// Construct a store of the Git repository at the given location. The location is assumed to
    /// be valid for the lifetime of this store: in particular, if the location is given as a
    /// relative path, then the current directory should not be changed.
    pub fn new(repo: PathBuf) -> GitStore {
        GitStore {
            directory: repo,
            commits: HashMap::new(),
        }
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

    /// Read details of a commit object (maybe from cache). The `hash` may be any commit reference:
    /// e.g., a literal unambiguous hash, a spec like `HASH~1^2` (where `HASH` is a full hash), or
    /// a context-sensitive reference like `HEAD` or `master`. It should not be misinterpretable as
    /// a flag to `git-show` or friends (e.g., anything starting with a hyphen).
    // TODO(@wchargin): A recent version of Git (which?) added `--end-of-options` to function like
    // `--` in commands where that is used to disambiguate revisions and paths. Use that to drop
    // the side condition?
    pub fn commit(&mut self, hash: &str) -> err::Result<&Commit> {
        if self.commits.contains_key(hash) {
            return Ok(self
                .commits
                .get(hash)
                .expect("hash not in map even after check"));
        }
        let commit = match self.read_commit(hash)? {
            ReadCommit::Cached(oid) => {
                return Ok(self
                    .commits
                    .get(&oid)
                    .expect("allegedly cached commit not in map"))
            }
            ReadCommit::Read(commit) => commit,
        };
        use std::collections::hash_map::Entry::{Occupied, Vacant};
        match self.commits.entry(commit.oid.clone()) {
            Occupied(e) => {
                let existing = e.into_mut();
                assert_eq!(&commit, existing);
                Ok(existing)
            }
            Vacant(e) => Ok(e.insert(commit)),
        }
    }

    fn read_commit(&self, hash: &str) -> err::Result<ReadCommit> {
        let show_output = self
            .git()
            .args(&["show", "--no-patch", "--pretty=format:%B%n%P%n%T%n%H", hash])
            .output()?;
        if !show_output.status.success() {
            return Err(err::Error::NoSuchCommit(hash.to_string()));
        }
        let mut stdout = err::Error::require_utf8(show_output.stdout, hash)?;
        let find_last_newline = |stdout: &mut String| {
            stdout
                .rfind('\n')
                .ok_or_else(|| err::Error::NoSuchCommit(hash.to_string()))
        };
        let split_off_at = |stdout: &mut String, i: usize| {
            let remainder = stdout.split_off(i + 1);
            stdout.pop().unwrap();
            remainder
        };
        let pre_hash_newline = find_last_newline(&mut stdout)?;
        let output_hash = split_off_at(&mut stdout, pre_hash_newline);
        if self.rev_parse_commit_ok(&output_hash)? != output_hash {
            // successfully showed a different kind of object, like a tree
            return Err(err::Error::NoSuchCommit(hash.to_string()));
        }
        if hash != output_hash {
            if self.commits.contains_key(hash) {
                return Ok(ReadCommit::Cached(output_hash));
            }
        }
        let pre_tree_newline = find_last_newline(&mut stdout)?;
        let tree = split_off_at(&mut stdout, pre_tree_newline);
        let pre_parents_newline = find_last_newline(&mut stdout)?;
        let mut reverse_parents: Vec<String> = Vec::new();
        loop {
            match stdout.rfind(' ') {
                Some(i) if i > pre_parents_newline => {
                    let parent = split_off_at(&mut stdout, i);
                    reverse_parents.push(parent);
                }
                _ => {
                    let parent = split_off_at(&mut stdout, pre_parents_newline);
                    if !parent.is_empty() {
                        // will be empty if there are no parents
                        reverse_parents.push(parent);
                    }
                    break;
                }
            }
        }
        let parents = {
            reverse_parents.reverse();
            reverse_parents
        };
        let message = stdout;
        Ok(ReadCommit::Read(Commit {
            oid: output_hash,
            parents,
            tree,
            message,
        }))
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
