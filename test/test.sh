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
    git -C client commit -m 'Create "b"' && tick
    >client/c printf 'c\n'
    git -C client add c
    git -C client commit -m 'Create "c"' && tick
    git -C client push origin work
    EDITOR='perl -i -pe "s/pick/edit/"' git -C client rebase -i HEAD~2
    >client/b printf 'b2\n'
    git -C client add b
    git -C client commit --amend --no-edit && tick
    git -C client rebase --continue
    >client/c printf 'c2\n'
    git -C client add c
    git -C client commit --amend --no-edit && tick
    git -C client rebase --continue
    git -C client log --color --graph work
    git -C client log --color --graph origin/work
    git -C client checkout --detach origin/work~
    commit="$(
        printf '[update patch]\n' |
            git -C client commit-tree work~^{tree} -p HEAD
    )" && tick
    git -C client checkout --detach origin/work
    if ! git -C client merge "${commit}" --no-edit -m '[diffbase]'; then
        git -C client add .
        git -C client commit --no-edit && tick
    fi
    commit="$(
        printf '[update patch]\n' |
            git -C client commit-tree work^{tree} -p HEAD
    )" && tick
    git -C client checkout --detach "${commit}"
    git -C client push origin HEAD:work
    git -C client log --format='%h %d %s' --graph origin/work
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
