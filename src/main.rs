use std::process::{Command, Stdio};

const BRANCH_DIRECTIVE: &str = "wchargin-branch";
const SOURCE_DIRECTIVE: &str = "wchargin-source";
const BRANCH_PREFIX: &str = "wchargin-";
const DEFAULT_REMOTE: &str = "origin";

mod err {
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

    impl From<std::io::Error> for Error {
        fn from(e: std::io::Error) -> Error {
            Error::IoError(e)
        }
    }

    pub type Result<T> = std::result::Result<T, Error>;
}

fn main() -> err::Result<()> {
    let oid =
        rev_parse("HEAD^{commit}")?.ok_or_else(|| err::Error::NoSuchCommit("HEAD".to_string()))?;
    let result = integrate(&oid)?;
    eprintln!("successfully integrated");
    println!("{}", result);
    match Command::new("git")
        .args(&["checkout", "--detach", &oid])
        .output()?
    {
        ref out if out.status.success() => (),
        _ => {
            return Err(err::Error::GitContract(format!(
                "failed to check out original commit {}",
                oid
            )))
        }
    };
    Ok(())
}

/// Process the change at `oid` to create a remote-friendly commit, returning the new commit's OID.
/// The new commit will be treequal to the input commit, and may be cleanly pushed to its remote
/// branch.
///
/// The diff of the commit at `oid` should represent the full contents of the change, and its
/// unique parent commit should be the desired diffbase.
///
/// The resulting commit will also be checked out on success. On failure, the state of the work
/// tree and index are not defined.
fn integrate(change_oid: &str) -> err::Result<String> {
    // Steps:
    //  1. Check out the remote version of the target, or (if none exists) the remote version of
    //     the parent, or (if none exists) the parent.
    //  2. Merge in the remote version of the parent, or (if none exists) the parent. Commit
    //     conflicts as they stand. Create an "update diffbase" commit if this incurs any changes.
    //  3. Commit the tree of the target. Create an "update patch" commit if this incurs any
    //     changes.
    //
    // Future enhancements:
    //
    //  4. If neither (2) nor (3) incurs changes, create a "CI bump" commit if so directed.
    //  5. If neither (2) nor (3) nor (4) incurs changes, create a "CI skip" commit, purely for
    //     updating the dx-source trailer reference.

    let change_branch = branch_name(&change_oid)?.ok_or_else(|| err::Error::MissingTrailer {
        oid: change_oid.to_string(),
        key: BRANCH_DIRECTIVE.to_string(),
    })?;

    let trailers = format!(
        "{}: {}\n{}: {}",
        BRANCH_DIRECTIVE,
        &change_branch[BRANCH_PREFIX.len()..], // hack
        SOURCE_DIRECTIVE,
        change_oid
    );

    let parent_oid_ref = format!("{}~^{{commit}}", change_oid);
    let parent_oid = match rev_parse(&parent_oid_ref)? {
        Some(v) => v,
        None => return Err(err::Error::NoSuchCommit(parent_oid_ref)),
    };
    let parent_branch = branch_name(&parent_oid)?; // may be absent

    let diffbase = match parent_branch {
        Some(ref name) => remote_branch_oid(DEFAULT_REMOTE, name)?,
        None => None,
    }
    .unwrap_or_else(|| parent_oid.clone());
    let merge_head =
        remote_branch_oid(DEFAULT_REMOTE, &change_branch)?.unwrap_or_else(|| diffbase.clone());

    // (1)
    let out = Command::new("git")
        .args(&["checkout", "--detach", &merge_head])
        .output()?;
    if !out.status.success() {
        let msg = format!(
            "failed to check out {}: {}",
            merge_head,
            String::from_utf8_lossy(&out.stderr)
        );
        return Err(err::Error::GitContract(msg));
    }

    // (2)
    let out = Command::new("git")
        .args(&[
            "-c",
            "rerere.enabled=false",
            "merge",
            "--no-edit",
            &diffbase,
            "-m",
            "[update diffbase]",
            "-m",
            &trailers,
        ])
        .output()?;
    if !out.status.success() {
        // Assume that this is due to conflicts.
        match Command::new("git").args(&["add", "."]).output()? {
            ref out if out.status.success() => (),
            out => {
                return Err(err::Error::GitContract(format!(
                    "failed to stage: {}",
                    String::from_utf8_lossy(&out.stderr)
                )))
            }
        };
        match Command::new("git")
            .args(&["commit", "--no-edit"])
            .output()?
        {
            ref out if out.status.success() => (),
            out => {
                return Err(err::Error::GitContract(format!(
                    "failed to commit merge: {}",
                    String::from_utf8_lossy(&out.stderr)
                )))
            }
        };
    }

    let base_tree = rev_parse("HEAD^{tree}")?
        .ok_or_else(|| err::Error::GitContract("failed to rev-parse HEAD^{tree}".to_string()))?;
    let change_tree_ref = format!("{}^{{tree}}", change_oid);
    let change_tree = rev_parse(&change_tree_ref)?.ok_or_else(|| {
        err::Error::GitContract(format!("failed to rev-parse {}", change_tree_ref))
    })?;

    // (3)
    let result = if change_tree != base_tree {
        let mut child = Command::new("git")
            .args(&["commit-tree", &change_tree, "-p", "HEAD"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.as_mut().expect("failed to open stdin");
        use std::io::Write;
        stdin.write_all(format!("[update patch]\n\n{}", &trailers).as_bytes())?;
        let out = child.wait_with_output()?;
        let result = parse_oid(out.stdout).map_err(|buf| {
            err::Error::GitContract(format!(
                "commit-tree gave bad output: {:?}",
                String::from_utf8_lossy(&buf),
            ))
        })?;
        match Command::new("git")
            .args(&["checkout", "--detach", &result])
            .output()?
        {
            ref out if out.status.success() => (),
            out => {
                return Err(err::Error::GitContract(format!(
                    "failed to commit merge: {}",
                    String::from_utf8_lossy(&out.stderr)
                )))
            }
        };
        result
    } else {
        rev_parse("HEAD")?
            .ok_or_else(|| err::Error::GitContract(("failed to rev-parse HEAD").to_string()))?
    };

    Ok(result)
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

fn trailers(message: String) -> err::Result<Vec<(String, String)>> {
    let mut comm = Command::new("git")
        .args(&[
            "-c",
            // TODO(@wchargin): Remove this explicit separator definition, in favor of the more
            // robust parsing algorithm described here:
            // https://public-inbox.org/git/CAFW+GMDazFSDzBrvzMqaPGwew=+CP7tw7G5FfDqcAUYd3qjPuQ@mail.gmail.com/
            "trailer.separators=:",
            "interpret-trailers",
            "--parse",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    use std::io::Write;
    comm.stdin.as_mut().unwrap().write_all(message.as_bytes())?;
    let out = comm.wait_with_output()?;
    let mut result = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let parts: Vec<_> = line.splitn(2, ": ").collect();
        if parts.len() != 2 {
            return Err(err::Error::GitContract(format!(
                "interpret-trailers emitted line: {:?}",
                line,
            )));
        }
        result.push((parts[0].to_string(), parts[1].to_string()));
    }
    Ok(result)
}

#[derive(Debug)]
enum TrailerMatch<'a> {
    Missing { key: &'a str },
    Duplicate { key: &'a str },
    Unique { key: &'a str, value: &'a str },
}

impl<'a> TrailerMatch<'a> {
    fn plus(self, value: &'a str) -> Self {
        use TrailerMatch::{Duplicate, Missing, Unique};
        match self {
            Missing { key } => Unique { key, value },
            Unique { key, .. } => Duplicate { key },
            Duplicate { .. } => self,
        }
    }
    fn unique(self, oid: &str) -> err::Result<&'a str> {
        match self {
            TrailerMatch::Unique { value, .. } => Ok(value),
            TrailerMatch::Missing { key } => Err(err::Error::MissingTrailer {
                oid: oid.to_string(),
                key: key.to_string(),
            }),
            TrailerMatch::Duplicate { key } => Err(err::Error::DuplicateTrailer {
                oid: oid.to_string(),
                key: key.to_string(),
            }),
        }
    }
    fn is_duplicate(&self) -> bool {
        match self {
            TrailerMatch::Duplicate { .. } => true,
            _ => false,
        }
    }
}

fn look_up_trailer<'a>(key: &'a str, trailers: &'a [(String, String)]) -> TrailerMatch<'a> {
    let mut found = TrailerMatch::Missing { key };
    for (k, v) in trailers {
        if k == key {
            found = found.plus(v);
            if found.is_duplicate() {
                return found;
            }
        }
    }
    found
}

fn branch_name(oid: &str) -> err::Result<Option<String>> {
    let msg = commit_message(&oid)?;
    let all_trailers = trailers(msg)?;
    match look_up_trailer(BRANCH_DIRECTIVE, &all_trailers).unique(&oid) {
        Ok(v) => Ok(Some(format!("{}{}", BRANCH_PREFIX, v))),
        Err(err::Error::MissingTrailer { .. }) => Ok(None),
        Err(other) => Err(other), // duplicate trailer
    }
}

fn remote_branch_oid(remote: &str, branch: &str) -> err::Result<Option<String>> {
    rev_parse(&format!("refs/remotes/{}/{}", remote, branch))
}

fn rev_parse(identifier: &str) -> err::Result<Option<String>> {
    let out = Command::new("git")
        .args(&["rev-parse", "--verify", identifier])
        .output()?;
    if !out.status.success() {
        return Ok(None);
    }
    parse_oid(out.stdout).map(Some).map_err(|buf| {
        err::Error::GitContract(format!(
            "rev-parse returned success but stdout was: {:?}",
            String::from_utf8_lossy(&buf)
        ))
    })
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
