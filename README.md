# git-dx

*Locally linear history:* Transparently integrate a local patch-based workflow
with a remote one-branch-per-change model.

## Purpose

Consider a stack of three commits representing a sequence of dependent changes:

    * (HEAD) Remove old APIs
    * Replace remaining users of old APIs with new APIs
    * Add new APIs
    * (origin/master) Delete failing tests

We wish to send these out for review, and we wish for the review periods to be
open simultaneously: no waiting for the first commit to be merged before sending
out the second for review. (Of course, the second commit cannot actually be
merged into master until the first one is.)

This is a simple task in *patch-based workflows*. Projects like the Linux kernel
and Git itself review code by sending raw patches to a mailing list, where
updates to a commit are sent as replies to the thread. Systems like Phabricator
and Gerrit similarly use *diffs* or *changelists* as the fundamental unit of
change. But the task is harder to accomplish in systems that assume *one branch
per change*, like GitHub and GitLab.

A simple approach is to push the first commit to a branch called `add-new-apis`,
the second to a branch `replace-users`, and the third to `remove-old-apis`. Open
three pull requests:

  - PR #1 wants to merge `add-new-apis` into `master`.
  - PR #2 wants to merge `replace-users` into `add-new-apis` (note: not
    `master`).
  - PR #3 wants to merge `remove-old-apis` into `replace-users`.

Once the first pull request is merged, we change the base branch of the second
pull request from `add-new-apis` to `master`. Once that one merges, we likewise
change the base of the third pull request from `replace-users` to `master`.

This is straightforward until we need to handle changes made during the review
of the first pull request. We may modify the first local commit in an
interactive rebase, which rewrites the subsequent two commits. The simplest way
forward is to force-push each commit to its respective target branch after
editing any commit in the sequence. This gets the job done, but has downsides
broader than the general warnings against force-pushing to a shared repository.

After force-pushing to GitHub, some things just don’t work. Code review comments
no longer properly hyperlink to the code in question. Notifications redirect to
404s and require manual dismissal. The pull request timeline events can collapse
consecutive force-pushes, with no apparent way to view the intermediate states.

Furthermore, after pushing any commit to GitHub, any references in that commit’s
headline or body will immediately be cross-linked. This includes numeric issue
references (`#123`), URL references, and username references (`@account`). Thus,
pushing multiple distinct commits with the same message causes both notification
spam and issue timeline spam. The notification spam is annoying, but transient;
the issue timeline spam persists forever, and can make it quite difficult to
navigate.

Thus, we adopt the following constraints.

First, we do not force-push. After making changes to a commit, we merge that
branch into the branches of any downstream pull requests and send fast-forward
updates to all of them. This is tedious to manage manually, especially while
maintaining a locally linear history, which is why `git-dx` exists.

Second, we only push a human-authored commit message once per pull request; any
further automated commits to the branch will have only short messages that
should not contain cross-references.

The local commit message is still the source of truth for all information about
the change. Its headline and body may periodically be used to update the title
and body of the corresponding *pull request*. Automated commits to the branch
will include a hash reference to the original commit. This will point to an
object known only to the local repository, and is provided as a convenience and
to make it harder to accidentally lose the source commit.

## Terminology

Two commits are **treequal** if their trees are equal. (The tree of a commit can
be found with `git rev-parse --verify COMMIT^{tree}`, replacing `COMMIT` with
the commit hash or other unique identifier. The tree of a commit describes the
full state of the repository’s content at that commit, but not the commit
history or metadata.)

A local commit in a linear history corresponding to a single change is called a
**source**, or **source commit**.

A source commit must have a **branch directive**, which is a Git trailer whose
value is the **branch key**. Prepending an optional **branch prefix** to the branch
key gives the **target branch name**. For instance, a commit with trailer

    Dx-branch: reticulate-splines

would specify a target branch name of `myname-reticulate-splines` if the branch
prefix were configured as `myname-`. See `man git-interpret-trailers` for more
information about trailers in general.

The remote branch specified by a source commit is called the **target branch**.
After a successful integration, the source commit and the head of the target
branch will be treequal. The target branch should be specified as the head
branch of the pull request.

The unique parent commit of a source commit is called the **local diffbase**.

The **remote diffbase** is the commit that should appear at the head of the
remote branch used as the “base branch” of the pull request. The right choice
for this commit is slightly fuzzy. If the local diffbase specifies a target
branch and is not itself an ancestor of `origin/master`, then the remote
diffbase is the head of the local diffbase’s target branch. Otherwise, the
remote diffbase is simply the local diffbase. In any case, the remote diffbase
should be treequal to the local diffbase.

## Status

In development. Not production-ready. No guarantees are made.

## Bugs

If you have set the `trailer.separators` config value to a set that does not
contain a colon, then trailers may not be set correctly.

## References

  - [Advice for clean history][linus] (Linus Torvalds, 2009-03-29, posted to the
    `dri-devel@lists.sourceforge.net` mailing list) ([mirror at
    mail-archive.com][linus-mailarchive])
  - [“Managing dependent pull requests”][mdpr] (Willow Chargin, 2017-07-28)

[linus-mailarchive]: https://www.mail-archive.com/dri-devel@lists.sourceforge.net/msg39091.html
[linus]: https://sourceforge.net/p/dri/mailman/message/21962376/
[mdpr]: https://wchargin.github.io/posts/managing-dependent-pull-requests
