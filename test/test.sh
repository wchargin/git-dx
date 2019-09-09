#!/bin/sh
set -eu

run=0
passed=0
failed=0

workdir=

set_up() {
    cd "$(mktemp -d)"
    export EDITOR=false
    export GIT_AUTHOR_EMAIL='author@example.com'
    export GIT_AUTHOR_NAME='A U Thor'
    export GIT_COMMITTER_EMAIL='committer@example.com'
    export GIT_COMMITTER_NAME='C O Mitter'
    export GIT_CONFIG_NOSYSTEM=1
    export GIT_MERGE_AUTOEDIT=no
    export GIT_MERGE_VERBOSITY=5
    export HOME="${PWD}"
    unset GIT_DIR
    unset GIT_WORK_TREE
    unset XDG_CACHE_HOME
    unset XDG_CONFIG_HOME
    tick
}

tick_number=1546300800

tick() {
    export GIT_AUTHOR_DATE="${tick_number} +0000"
    export GIT_COMMITTER_DATE="${tick_number} +0000"
    : $(( tick_number += 60 ))
}

test_basic() {
    git init --quiet --bare server
    git init --quiet client
    git -C client remote add origin "${PWD}/server"

    >client/a printf 'a\n'
    git -C client add a
    git -C client commit -m 'Create "a"' && tick
    git -C client push origin master
    git -C client checkout -b work

    >client/b printf 'b\n'
    git -C client add b
    git -C client commit -m 'Create "b"' -m 'wchargin-branch: foo' && tick
    git -C client push origin HEAD:refs/heads/wchargin-foo

    >client/c printf 'c\n'
    git -C client add c
    git -C client commit -m 'Create "c"' -m 'wchargin-branch: bar' && tick
    git -C client push origin HEAD:refs/heads/wchargin-bar

    EDITOR='perl -i -pe "s/pick/edit/"' git -C client rebase -i HEAD~2

    >client/b printf 'b2\n'
    git -C client add b
    git -C client commit --amend -m 'Amend "b"' -m 'wchargin-branch: foo' && tick
    dx_commit="$(git -C client dx)"
    git -C client push origin "${dx_commit}":wchargin-foo
    git -C client rebase --continue

    >client/c printf 'c2\n'
    git -C client add c
    git -C client commit --amend -m 'Amend "c"' -m 'wchargin-branch: bar' && tick
    dx_commit="$(git -C client dx)"
    git -C client push origin "${dx_commit}":wchargin-bar
    git -C client rebase --continue

    >client/d printf 'd\n'
    git -C client add d
    git -C client commit -m 'Create "d"' -m 'wchargin-branch: baz' && tick
    dx_commit="$(git -C client dx)"
    git -C client push origin "${dx_commit}":refs/heads/wchargin-baz
    git -C client checkout work

    git -C client log --color --oneline --graph work
    git -C client log --color --oneline --graph origin/wchargin-baz

    foo_local_tree="$(git -C client rev-parse --verify 'work~2^{tree}')"
    bar_local_tree="$(git -C client rev-parse --verify 'work~1^{tree}')"
    baz_local_tree="$(git -C client rev-parse --verify 'work~0^{tree}')"
    foo_remote_tree="$(git -C server rev-parse --verify 'wchargin-foo^{tree}')"
    bar_remote_tree="$(git -C server rev-parse --verify 'wchargin-bar^{tree}')"
    baz_remote_tree="$(git -C server rev-parse --verify 'wchargin-baz^{tree}')"
    [ "${foo_local_tree}" = "${foo_remote_tree}" ]
    [ "${bar_local_tree}" = "${bar_remote_tree}" ]
    [ "${baz_local_tree}" = "${baz_remote_tree}" ]
}

run_test_case() {
    set +e
    (set_up; set -ex; "$1") >out 2>&1
    result=$?
    set -e
    : $(( run += 1 ))
    if [ "${result}" -ne 0 ] ;then
        tput bold
        tput setaf 1
        printf 'FAIL'
        tput sgr0
        printf ' %s exited %d\n' "$1" "${result}"
        : $(( failed += 1 ))
        cat out
    else
        tput bold
        printf 'PASS'
        tput sgr0
        printf ' %s\n' "$1"
        : $(( passed += 1 ))
    fi
}

run_test_cases() {
    run_test_case test_basic
}

main() {
    if [ $# -ne 1 ]; then
        printf >&2 'usage: %s GIT_DX_BINARY\n' "$0"
        return 2
    fi
    case "$1" in
        /*) export GIT_DX_BINARY="$1" ;;
        *) export GIT_DX_BINARY="${PWD}/$1" ;;
    esac
    if ! [ -x "${GIT_DX_BINARY}" ]; then
        printf >&2 'fatal: expected GIT_DX_BINARY to be executable: %s\n' \
            "${GIT_DX_BINARY}"
        return 2
    fi
    trap cleanup EXIT
    workdir="$(mktemp -d)"
    cd "${workdir}"
    mkdir bin
    export PATH="${PWD}/bin:${PATH}"
    ln -s "${GIT_DX_BINARY}" ./bin/git-dx
    export TMPDIR="$PWD"
    run_test_cases
    printf '%s run, %s passed, %s failed\n' "${run}" "${passed}" "${failed}"
    [ "${failed}" -eq 0 ]
}

cleanup() {
    if [ -n "${workdir}" ]; then
        rm -rf "${workdir}"
    fi
}

main "$@"
