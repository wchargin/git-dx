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
    git -C client commit --allow-empty -m 'Initial commit' && tick
    git -C client push origin master
    git -C client rev-parse --verify 'HEAD^{commit}'
    git -C server rev-parse --verify 'HEAD^{commit}'
}

run_test_case() {
    result=0
    (set_up; set -x; "$1") || result=$?
    : $(( run += 1 ))
    if [ "${result}" -ne 0 ] ;then
        tput bold
        tput setaf 1
        printf 'FAIL'
        tput sgr0
        printf ' %s exited %d\n' "$1" "${result}"
        : $(( failed += 1 ))
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
