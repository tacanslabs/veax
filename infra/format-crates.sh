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
Perform crates formatting in specified directory subtree,
or check if they're formatted correctly in check mode

Usage:
  $(basename $0) [<PATH>] [opts...]

Where:
  <PATH>            - path to working directory where to start lookup;
                      current working directory assumed if not specified

  --help            - print this help message and exit with failure

  --halt-on <PHASE> - specify phase after which script must stop if error encountered
                      - none  - process all encountered crates
                      - crate - after first crate with failure
                                default

  --here            - try run formatter only in working directory, without running lookup;
                      this will fail if there's no crate in WD

  --check           - only check that crates are formatted, without doing actual formatting
EOF
}
# Result code for tasks being executed
RESULT=0

WORKDIR=$(pwd)
MODE=fix
WALKDIR=1
# 0 - don't halt
# 1 - after crate
HALTMODE=1

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
  # Skip directories which contain file named skip.format
  if [[ -e "$1/skip.format" ]] ; then
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
# Convert path to one relative to repo's root
#
# $1 - path to convert
rel-to-root() {
  realpath --relative-to="${ROOT_DIR}" $1
}
# Perform formatting or format check
format-crate() {
  case ${MODE} in
    fix)
      cargo fmt
    ;;
    check)
      # NB: we lose error code here for some reason, so we use output
      local OUTP=$(cargo fmt --check -- -l)
      if [[ -z "${OUTP}" ]] ; then
        return 0
      fi

      for i in ${OUTP} ; do
        echo "  M $(rel-to-root $i)"
      done

      return 1
    ;;
    *)
      echo "Unknown mode ${MODE}"
      exit 1
    ;;
  esac
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
  exec-level 1 format-crate
  set -eo pipefail

  popd >/dev/null
done

popd >/dev/null
exit ${RESULT}
