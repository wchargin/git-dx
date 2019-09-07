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
    git init --quiet
}

test_basic() {
    [ -d .git ]
    pwd
}

run_test_case() {
    result=0
    (set_up; set -x; "$1") || result=$?
    (( ++run ))
    if [ "${result}" -ne 0 ] ;then
        tput bold
        tput setaf 1
        printf 'FAIL'
        tput sgr0
        printf ' %s exited %d\n' "$1" "${result}"
        (( ++failed ))
    else
        tput bold
        printf 'PASS'
        tput sgr0
        printf ' %s\n' "$1"
        (( ++passed ))
    fi
}

main() {
    trap cleanup EXIT
    workdir="$(mktemp -d)"
    cd "${workdir}"
    export TMPDIR="$PWD"
    run_test_case test_basic
    printf '%s run, %s passed, %s failed\n' "${run}" "${passed}" "${failed}"
    [ "${failed}" -eq 0 ]
}

cleanup() {
    if [ -n "${workdir}" ]; then
        rm -r "${workdir}"
    fi
}

main "$@"
