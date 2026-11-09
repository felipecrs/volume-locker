#!/usr/bin/env bash
# Runs the end-to-end tests in a disposable Windows VM, building its image first if there isn't one.
# Cargo calls it as the test runner (see .cargo/config.toml). Needs Docker and KVM.
# Usage: run.sh <VolumeLocker.exe> <test exe> [test arguments...]
set -euo pipefail

E2E_HOME=${E2E_HOME:-$HOME/.cache/volume-locker-e2e}
E2E_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
STORAGE=$E2E_HOME/windows
CONTAINER=volume-locker-e2e
IMAGE=docker.io/dockurr/windows@sha256:16fd7c39a3e48bb946d29c59dc6015942e5b900a1fa5d7e5552053bfc48ee2ea
INSTALL_TIMEOUT=7200
BOOT_ATTEMPTS=3
BOOT_TIMEOUT=300
TEST_TIMEOUT=1200

# Creates and boots the VM. Extra arguments go to `docker run`.
boot_vm() {
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
  # Every boot must pass the same VERSION, or dockur moves the image to backups/ and reinstalls.
  docker run --detach --name "$CONTAINER" \
    --env VERSION=11l --env RAM_SIZE=4G --env CPU_CORES=4 --env DISK_SIZE=32G \
    --publish 127.0.0.1:8006:8006 \
    --device /dev/kvm --device /dev/net/tun --cap-add NET_ADMIN \
    --volume "$STORAGE:/storage" --volume "$shared:/shared" \
    --stop-timeout 120 "$@" "$IMAGE" >/dev/null
}

# Waits until the VM writes `file` to the shared folder. Fails on timeout or if the VM stops first.
wait_for_file() {
  local file=$1 timeout=$2 elapsed=0
  until [ -f "$shared/$file" ]; do
    if [ "$(docker inspect --format '{{.State.Running}}' "$CONTAINER")" != true ]; then
      echo "The VM stopped before writing $file. Its last log lines:" >&2
      docker logs --tail 20 "$CONTAINER" >&2 2>&1
      return 1
    fi
    if [ "$elapsed" -ge "$timeout" ]; then
      echo "The VM did not write $file within ${timeout}s." >&2
      return 1
    fi
    sleep 5
    elapsed=$((elapsed + 5))
  done
}

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

mkdir -p "$E2E_HOME"
shared=$(mktemp -d "$E2E_HOME/shared.XXXXXX")
chmod a+rwx "$shared"
trap 'docker rm -f "$CONTAINER" >/dev/null 2>&1 || true; rm -rf "$shared"' EXIT

# To rebuild the image, delete $E2E_HOME.
if [ ! -f "$STORAGE/windows.boot" ]; then
  echo "Building the Windows image first, which takes 10 to 30 minutes. Watch it at http://localhost:8006"
  rm -rf "$STORAGE"
  mkdir -p "$STORAGE"
  # Dockur copies oem/ to C:\OEM and runs install.bat at the end of setup.
  boot_vm --volume "$E2E_DIR/oem:/oem:ro"
  # oem/logon.cmd writes this and powers off at the first logon, since no tests are in the shared folder.
  wait_for_file provisioned "$INSTALL_TIMEOUT"
  timeout 300 docker wait "$CONTAINER" >/dev/null
  rm "$shared/provisioned"
fi

install -m 777 "$tests" "$shared/e2e.exe"
install -m 777 "$app" "$shared/VolumeLocker.exe"
# One line, since logon.cmd reads only the first.
echo "$*" >"$shared/test-args"

# A boot occasionally hangs before reaching the desktop, so try again.
for ((attempt = 1; attempt <= BOOT_ATTEMPTS; attempt++)); do
  # -snapshot throws away everything the VM writes to its disk, so every run starts clean.
  boot_vm --env ARGUMENTS=-snapshot
  if wait_for_file started "$BOOT_TIMEOUT"; then
    status=0
    wait_for_file exit-code "$TEST_TIMEOUT" || status=1
    sed 's/\r$//' "$shared/tests.log" 2>/dev/null || true
    [ "$status" -eq 0 ] || exit 1
    exit "$(tr -dc '0-9' <"$shared/exit-code")"
  fi
done
echo "The VM did not boot after $BOOT_ATTEMPTS attempts." >&2
exit 1
