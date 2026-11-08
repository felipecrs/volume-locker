#!/usr/bin/env bash
# Runs the end-to-end tests in a disposable VM. Usage: run.sh <test exe> <VolumeLocker.exe>
set -euo pipefail
source "$(dirname "$0")/lib.sh"

BOOT_ATTEMPTS=3
BOOT_TIMEOUT=300
TEST_TIMEOUT=1200

if [ ! -f "$STORAGE/windows.boot" ]; then
  echo "No base image in $STORAGE. Run provision.sh first." >&2
  exit 1
fi

shared=$(new_shared_folder)
trap 'docker rm -f "$CONTAINER" >/dev/null 2>&1 || true; rm -rf "$shared"' EXIT
install -m 777 "$1" "$shared/e2e.exe"
install -m 777 "$2" "$shared/VolumeLocker.exe"

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
