#!/bin/bash
set -eo pipefail
#
# Locate script's directory and repo's root
#
SELF_DIR=$(readlink -f "$(dirname "$0")")
ROOT_DIR=$(readlink -f "${SELF_DIR}/..")
#
# CLI
#
usage() {
  cat << EOF
Run linter over all crates found in directory subtree and try fix errors, if possible

Usage:
  $(basename $0) [<PATH>] [opts...]

Where:
  <PATH>            - path to working directory where to start lookup;
                      current working directory assumed if not specified

  --help            - print this help message and exit with failure

  --halt-on <PHASE> - specify phase after which script must stop if error encountered
                      - none    - process all encountered crates
                      - crate   - after first crate with failure
                      - params  - after first parameter set which failed
                                  default

  --here            - try run linter only in working directory, without running lookup;
                      this will fail if there's no crate in WD

  --check           - run in check-only mode, i.e. don't try to fix linting issues
EOF
}
# Result code for tasks being executed
RESULT=0

WORKDIR=$(pwd)
MODE=fix
WALKDIR=1
# 0 - don't halt
# 1 - after crate
# 2 - after params
HALTMODE=2

while [[ $# -gt 0 ]] ; do
  case $1 in
    --help)
      usage
      exit 1
    ;;
    --halt-on)
      if [[ $# -eq 1 ]] ; then
        echo "--halt-on: missing parameter"
        exit 1
      fi
      shift
      case $1 in
        none)
          HALTMODE=0
        ;;
        crate)
          HALTMODE=1
        ;;
        params)
          HALTMODE=2
        ;;
        *)
          echo "Unknown halt mode: $1"
          exit 1
        ;;
      esac
      shift
    ;;
    --here)
      WALKDIR=0
      shift
    ;;
    --check)
      MODE=check
      shift
    ;;
    -*|--*)
      echo "Unknown option $1"
      exit 1
    ;;
    *)
      WORKDIR=$(readlink -f $1)
      shift
    ;;
  esac
done

#
# Service funcs
#

find-crates () {
  # Skip directories which contain file named skip.lint
  if [[ -e "$1/skip.lint" ]] ; then
    return
  fi
  # Print directory, if it has Cargo.toml embedded
  if [[ -e "$1/Cargo.toml" ]] ; then
    echo "$1"
  fi

  for entry in $1/* ; do
    local name=$(basename "${entry}")
    # Skip files and hidden entries
    if [[ ! -d "${entry}" || ${name} == .*  || ${name} == target ]] ; then
      continue
    fi
    # Recurse
    find-crates "${entry}"
  done
}
# Execute task, collecting its exit code,
# and halt script if execution level is lower than halt mode
exec-level() {
  local LEVEL=$1
  shift
  $*
  local CALL_RESULT=$?
  RESULT=$((${RESULT} + ${CALL_RESULT}))
  if [[ ${HALTMODE} -ge ${LEVEL} && ! ${RESULT} -eq 0  ]] ; then
    exit ${RESULT}
  fi
  return ${CALL_RESULT}
}
# Trims whitespaces from provided arguments
trim-ws() {
  set -- $*
  echo $*
}
# Convert path to one relative to repo's root
#
# $1 - path to convert
rel-to-root() {
  realpath --relative-to="${ROOT_DIR}" $1
}
# Read parameters from file,
# use specified default if file is missing,
# execute callback for each line of params
# If final params set is empty, function does nothing
#
# $1  - name of parameters file
# $2  - default parameters, quote if needed
# ... - callback expression, receives each params line added to its end
enum-params-file() {
  local ALL_PARAMS=""
  if [ -e "$1" ] ; then
    local ALL_PARAMS=$(cat "$1" 2>/dev/null)
  else
    local ALL_PARAMS=$2
  fi
  shift 2

  local CALL_RESULT=0
  while IFS= read -r PARAMS; do
    PARAMS="$(trim-ws ${PARAMS})"
    if [[ -z "${PARAMS}" || "${PARAMS}" = \#* ]] ; then
      continue
    fi
    echo "  params '${PARAMS}'..."
    $* ${PARAMS}
    local CALL_RESULT=$((${CALL_RESULT} + $?))
  done <<< "$ALL_PARAMS"
  return ${CALL_RESULT}
}

lint-crate-with-params() {
  local CLIPPY_EXTRA_ARGS=""
  if [[ "${MODE}" == fix ]] ; then
    local CLIPPY_EXTRA_ARGS="--fix --allow-dirty --allow-staged"
  fi
  cargo clippy -q --no-deps --tests --benches --examples $* ${CLIPPY_EXTRA_ARGS} -- \
    -W clippy::correctness \
    -W clippy::suspicious \
    -W clippy::style \
    -W clippy::complexity \
    -W clippy::perf \
    -W clippy::pedantic \
    \
    -A clippy::return_self_not_must_use \
    -A clippy::must_use_candidate \
    -A clippy::similar_names \
    -A clippy::missing_errors_doc \
    -A clippy::missing_panics_doc \
    -A clippy::module_name_repetitions \
    -A clippy::derive_partial_eq_without_eq \
    -A clippy::redundant_closure_for_method_calls
}

lint-crate() {
  enum-params-file project.lint "--all-features" exec-level 2 lint-crate-with-params
}

#
# Actual script
#

# Get into working directory and either perform lookup or set FILES to `./Cargo.toml`
pushd ${WORKDIR} >/dev/null

if [[ ${WALKDIR} -eq 1 ]] ; then
  CRATES=$(find-crates .)
else
  # Check only here, find will carry such check implicitly
  if [[ ! -e "./Cargo.toml" ]] ; then
    echo "$(rel-to-root $(pwd)): not a crate!"
    exit 1
  fi
  CRATES="."
fi

# Loop over crates
for i in ${CRATES} ; do
  pushd $i >/dev/null
  echo "$(rel-to-root $(pwd))"

  set +eo pipefail
  exec-level 1 lint-crate
  set -eo pipefail

  popd >/dev/null
done

popd >/dev/null
exit ${RESULT}
