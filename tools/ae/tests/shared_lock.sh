#!/usr/bin/env bash
# Tests the worker lock with shell commands and an isolated lock directory.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
AECTL="$HERE/../aectl"
test_dir=$(mktemp -d)
trap 'touch "$test_dir/release"; wait; rm -rf "$test_dir"' EXIT
export BAE_AE_LOCK_DIR="$test_dir/locks"
export AE_APP="$test_dir/missing.app"
lock="$BAE_AE_LOCK_DIR/ae.lock"

fail() { echo "FAIL: $*" >&2; exit 1; }
expect_busy() {
  local output
  if output=$("$AECTL" lock --lock-timeout 0 -- touch "$test_dir/unexpected" 2>&1); then
    fail "acquired a held lock"
  fi
  [[ "$output" == *"still reserved"* ]] || fail "wrong failure: $output"
  [[ ! -e "$test_dir/unexpected" ]] || fail "ran a command without acquiring the lock"
}

# Legacy workers may claim the path as a directory or a file.
mkdir -p "$lock"
expect_busy
rmdir "$lock"
echo legacy > "$lock"
expect_busy
[[ "$(cat "$lock")" == legacy ]] || fail "modified an external lock"
rm "$lock"

"$AECTL" lock -- bash -ec '
  touch "$1/started"
  while [[ ! -e "$1/release" ]]; do sleep 0.1; done
  touch "$1/finished"
' _ "$test_dir" &
first=$!
for _ in $(seq 1 50); do
  [[ ! -e "$test_dir/started" ]] || break
  sleep 0.1
done
[[ -f "$test_dir/started" ]] || fail "first worker did not start"
owner=$(cat "$lock/owner")
expect_busy
[[ "$(cat "$lock/owner")" == "$owner" ]] || fail "changed the current owner"

"$AECTL" lock --lock-timeout 10 -- bash -ec '
  [[ -f "$1/finished" && "$2" == --force ]]
  touch "$1/second"
' _ "$test_dir" --force 2>"$test_dir/wait.log" &
second=$!
for _ in $(seq 1 50); do
  [[ ! -s "$test_dir/wait.log" ]] || break
  sleep 0.1
done
grep -q 'waiting for the shared After Effects lock' "$test_dir/wait.log" || fail "second worker did not queue"
[[ ! -e "$test_dir/second" ]] || fail "second worker started before release"
touch "$test_dir/release"
wait "$first"
wait "$second"
[[ -f "$test_dir/second" && ! -e "$lock" ]] || fail "second worker did not run and release the lock"

rc=0
"$AECTL" lock -- bash -c 'exit 23' || rc=$?
[[ "$rc" -eq 23 && ! -e "$lock" ]] || fail "lost the command exit code or left a lock behind"
echo "PASS: shared AE lock"
