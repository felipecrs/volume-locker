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
  if wait_for_agent "$shared" "$BOOT_TIMEOUT"; then
    run_job "$shared" run-tests "$TEST_TIMEOUT"
    exit "$(cat "$shared/outbox/exit-code" 2>/dev/null || echo 1)"
  fi
done
echo "The VM did not boot after $BOOT_ATTEMPTS attempts." >&2
exit 1
