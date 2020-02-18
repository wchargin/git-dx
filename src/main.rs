extern crate clap;

use std::path::PathBuf;
use std::process::{Command, Stdio};

const BRANCH_DIRECTIVE: &str = "wchargin-branch";
const SOURCE_DIRECTIVE: &str = "wchargin-source";
const BRANCH_PREFIX: &str = "wchargin-";
const DEFAULT_REMOTE: &str = "origin";

mod err;
mod git;

use git::GitStore;

fn main() -> err::Result<()> {
    const CLI_ARG_COMMIT: &'static str = "commit";
    const CLI_ARG_DRY_RUN: &'static str = "dry_run";
    const CLI_ARG_PUSH: &'static str = "push";

    let mut git = GitStore::new(PathBuf::new());
    let matches = clap::App::new("git-dx")
        .version("0.1.0")
        .arg(
            clap::Arg::with_name(CLI_ARG_COMMIT)
                .help("Source commit")
                .required(true)
                .default_value("HEAD")
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name(CLI_ARG_PUSH)
                .help("Pushes integrated commit to remote")
                .long("--push"),
        )
        .arg(
            clap::Arg::with_name(CLI_ARG_DRY_RUN)
                .help("Use dry-run pushes only")
                .long("--dry-run")
                .short("-n"),
        )
        .get_matches();
    let source_commit = matches.value_of(CLI_ARG_COMMIT).unwrap();
    let push = matches.is_present(CLI_ARG_PUSH);
    let dry_run = matches.is_present(CLI_ARG_DRY_RUN);
    let oid = git.rev_parse_commit_ok(source_commit)?;
    let result = integrate(&mut git, &oid)?;
    eprintln!("successfully integrated");
    println!("{}", result.remote_commit);
    err::from_git(
        &Command::new("git")
            .args(&["checkout", "--detach", &oid])
            .output()?,
        || "failed to check out original commit".to_string(),
    )?;
    if push {
        let mut cmd = Command::new("git");
        cmd.arg("push");
        if dry_run {
            cmd.arg("--dry-run");
        }
        cmd.arg(DEFAULT_REMOTE);
        cmd.arg(&format!(
            "{}:refs/heads/{}",
            result.remote_commit, result.target_branch
        ));
        let push_output = cmd.output()?;
        err::from_git(&push_output, || "failed to push".to_string())?;
        eprint!("{}", String::from_utf8_lossy(&push_output.stdout));
        eprint!("{}", String::from_utf8_lossy(&push_output.stderr));
    }
    Ok(())
}

struct Integration {
    remote_commit: String,
    target_branch: String,
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
fn integrate(git: &mut git::GitStore, source_oid: &str) -> err::Result<Integration> {
    // Steps (see Terminology section of README.md):
    //
    //  1. Check out the remote target branch, or (if none exists) the remote diffbase, or (if none
    //     exists) the local diffbase.
    //  2. Merge in the remote diffbase, or (if none exists) the local diffbase. Commit conflicts
    //     as they stand. Create an "update diffbase" commit if this incurs any changes.
    //  3. Commit the tree of the source commit. Create an "update patch" commit if this incurs any
    //     changes.
    //
    // Future enhancements:
    //
    //  4. If neither (2) nor (3) incurs changes, create a "CI bump" commit if so directed.
    //  5. If neither (2) nor (3) nor (4) incurs changes, create a "CI skip" commit, purely for
    //     updating the dx-source trailer reference.
    let source_commit = git.commit(&source_oid)?.clone();

    let target_branch = branch_name(&source_oid, &source_commit.message)?.ok_or_else(|| {
        err::Error::MissingTrailer {
            oid: source_oid.to_string(),
            key: BRANCH_DIRECTIVE.to_string(),
        }
    })?;
    let target_branch_unprefixed = &target_branch[BRANCH_PREFIX.len()..]; // hack

    let remote_diffbase = {
        let local_diffbase = git.commit(&format!("{}~^{{commit}}", source_oid))?.clone();
        match branch_name(&local_diffbase.oid, &local_diffbase.message)? {
            Some(ref name) => remote_branch_oid(git, DEFAULT_REMOTE, name)?,
            None => None,
        }
        .unwrap_or_else(|| local_diffbase.oid)
    };
    let merge_head = remote_branch_oid(git, DEFAULT_REMOTE, &target_branch)?;
    let new_branch = merge_head.is_none();
    let merge_head = merge_head.unwrap_or_else(|| remote_diffbase.clone());

    // (1)
    let out = Command::new("git")
        .args(&["checkout", "--detach", &merge_head])
        .output()?;
    err::from_git(&out, || format!("failed to check out {}", merge_head))?;
    std::mem::drop(out);

    // (2)
    let out = Command::new("git")
        .args(&[
            "-c",
            "rerere.enabled=false",
            "merge",
            "--no-edit",
            &remote_diffbase,
            "-m",
            "[update diffbase]",
            "-m",
            &format!(
                "{}: {}\n{}: {}",
                BRANCH_DIRECTIVE, target_branch_unprefixed, SOURCE_DIRECTIVE, source_oid
            ),
        ])
        .output()?;
    if !out.status.success() {
        // Assume that this is due to conflicts.
        let out = &Command::new("git").args(&["add", "."]).output()?;
        err::from_git(out, || "failed to stage".to_string())?;
        let out = &Command::new("git")
            .args(&["commit", "--no-edit"])
            .output()?;
        err::from_git(out, || "failed to commit merge".to_string())?;
    }
    std::mem::drop(out);

    let base_commit = git
        .commit(
            // TODO(@wchargin): Let `GitStore::commit` operate on ephemeral refs like `HEAD` by
            // taking an argument indicating whether to cache both forms.
            &git.rev_parse("HEAD")?
                .ok_or_else(|| err::Error::GitContract(("failed to rev-parse HEAD").to_string()))?,
        )?
        .clone();

    // (3)
    let remote_commit = if source_commit.tree == base_commit.tree {
        base_commit.oid
    } else {
        let msg = if new_branch {
            source_commit.message
        } else {
            "[update patch]\n".to_string()
        };
        let mut interpret_trailers_child = Command::new("git")
            .args(&[
                "interpret-trailers",
                "--no-divider",
                "--where",
                "end",
                "--if-exists",
                "replace",
                "--trailer",
                &format!("{}: {}", BRANCH_DIRECTIVE, target_branch_unprefixed),
                "--trailer",
                &format!("{}: {}", SOURCE_DIRECTIVE, source_oid),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let commit_tree_child = Command::new("git")
            .args(&["commit-tree", &source_commit.tree, "-p", "HEAD"])
            .stdin(
                interpret_trailers_child
                    .stdout
                    .take()
                    .expect("interpret-trailers stdout"),
            )
            .stdout(Stdio::piped())
            .spawn()?;
        let stdin = interpret_trailers_child
            .stdin
            .as_mut()
            .expect("interpret-trailers-stdin");
        use std::io::Write;
        stdin.write_all(msg.as_bytes())?;
        interpret_trailers_child.wait()?;
        let out = commit_tree_child.wait_with_output()?;
        let result = git::parse_oid(out.stdout).map_err(|buf| {
            err::Error::GitContract(format!(
                "commit-tree gave bad output: {:?}",
                String::from_utf8_lossy(&buf),
            ))
        })?;
        let out = Command::new("git")
            .args(&["checkout", "--detach", &result])
            .output()?;
        err::from_git(&out, || "failed to commit merge".to_string())?;
        result
    };

    Ok(Integration {
        remote_commit,
        target_branch,
    })
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
            "--no-divider",
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

fn branch_name(oid: &str, msg: &str) -> err::Result<Option<String>> {
    let all_trailers = trailers(msg.to_string())?;
    match look_up_trailer(BRANCH_DIRECTIVE, &all_trailers).unique(&oid) {
        Ok(v) => Ok(Some(format!("{}{}", BRANCH_PREFIX, v))),
        Err(err::Error::MissingTrailer { .. }) => Ok(None),
        Err(other) => Err(other), // duplicate trailer
    }
}

fn remote_branch_oid(
    git: &mut git::GitStore,
    remote: &str,
    branch: &str,
) -> err::Result<Option<String>> {
    git.rev_parse(&format!("refs/remotes/{}/{}", remote, branch))
}
