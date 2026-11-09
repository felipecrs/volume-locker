#!/usr/bin/env bash
# Runs the end-to-end tests in a disposable VM. Cargo calls it as the test runner (see .cargo/config.toml).
# Usage: run.sh <VolumeLocker.exe> <test exe> [test arguments...]
set -euo pipefail
source "$(dirname "$0")/lib.sh"

# Cargo on Windows passes Windows paths.
linux_path() {
  if [[ "$1" == [A-Za-z]:\\* ]]; then wslpath -u "$1"; else echo "$1"; fi
}
app=$(linux_path "$1")
tests=$(linux_path "$2")
shift 2
if [ ! -f "$app" ]; then
  echo "No app at $app. Build it first, or use 'mise run e2e'." >&2
  exit 1
fi

BOOT_ATTEMPTS=3
BOOT_TIMEOUT=300
TEST_TIMEOUT=1200

if [ ! -f "$STORAGE/windows.boot" ]; then
  echo "No Windows image yet, so building one first. This takes 10 to 30 minutes."
  bash "$E2E_DIR/provision.sh"
fi

shared=$(new_shared_folder)
trap 'docker rm -f "$CONTAINER" >/dev/null 2>&1 || true; rm -rf "$shared"' EXIT
install -m 777 "$tests" "$shared/e2e.exe"
install -m 777 "$app" "$shared/VolumeLocker.exe"
install -m 666 "$E2E_DIR/run-tests.ps1" "$shared/run-tests.ps1"
printf '%s\n' "$@" >"$shared/test-args"

# A boot occasionally hangs before reaching the desktop, so try again.
for ((attempt = 1; attempt <= BOOT_ATTEMPTS; attempt++)); do
  # -snapshot throws away everything the VM writes to its disk, so every run starts clean.
  boot_vm "$shared" --env ARGUMENTS=-snapshot
  if wait_for_file "$shared/started" "$BOOT_TIMEOUT"; then
    status=0
    wait_for_file "$shared/exit-code" "$TEST_TIMEOUT" || status=1
    sed 's/\r$//' "$shared/tests.log" 2>/dev/null || true
    [ "$status" -eq 0 ] || exit 1
    exit "$(tr -dc '0-9' <"$shared/exit-code")"
  fi
done
echo "The VM did not boot after $BOOT_ATTEMPTS attempts." >&2
exit 1
