use std::process::{Command, Stdio};

const BRANCH_DIRECTIVE: &str = "wchargin-branch";
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
    let branch = look_up_trailer(BRANCH_DIRECTIVE, &trailers(commit_message(&oid)?)?)
        .unique(&oid)?
        .to_string();
    let remote_branch = format!("{}{}", BRANCH_PREFIX, &branch);
    let remote_oid = remote_branch_oid(DEFAULT_REMOTE, &remote_branch)?;
    println!("commit branch: {} -> {:?}", remote_branch, remote_oid);

    let parent_oid = rev_parse(&format!("{}~^{{commit}}", oid))?
        .ok_or_else(|| err::Error::NoSuchCommit("HEAD~".to_string()))?;
    let parent_branch: Option<String> =
        match look_up_trailer(BRANCH_DIRECTIVE, &trailers(commit_message(&parent_oid)?)?) {
            TrailerMatch::Duplicate { key } => {
                return Err(err::Error::DuplicateTrailer {
                    oid: parent_oid.to_string(),
                    key: key.to_string(),
                })
            }
            TrailerMatch::Missing { .. } => None,
            TrailerMatch::Unique { value, .. } => Some(value.to_string()),
        };
    let merge_target_oid = match parent_branch {
        None => parent_oid,
        Some(ref b) => remote_branch_oid(DEFAULT_REMOTE, &format!("{}{}", BRANCH_PREFIX, b))?
            .unwrap_or(parent_oid),
    };
    println!(
        "merge target: {:?} -> {:?}",
        parent_branch, merge_target_oid
    );
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
