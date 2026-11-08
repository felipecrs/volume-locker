# Shared by provision.sh and run.sh.
E2E_HOME=${E2E_HOME:-$HOME/.cache/volume-locker-e2e}
E2E_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
STORAGE=$E2E_HOME/windows
CONTAINER=volume-locker-e2e
IMAGE=docker.io/dockurr/windows@sha256:16fd7c39a3e48bb946d29c59dc6015942e5b900a1fa5d7e5552053bfc48ee2ea

# Creates and boots the VM. Extra arguments go to `docker run`.
boot_vm() {
  local shared=$1
  shift
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
  # Every boot must pass the same VERSION, or dockur moves the image to backups/ and reinstalls.
  docker run --detach --name "$CONTAINER" \
    --env VERSION=11l --env RAM_SIZE=4G --env CPU_CORES=4 --env DISK_SIZE=32G \
    --publish 127.0.0.1:8006:8006 \
    --device /dev/kvm --device /dev/net/tun --cap-add NET_ADMIN \
    --volume "$STORAGE:/storage" --volume "$shared:/shared" \
    --stop-timeout 120 "$@" "$IMAGE" >/dev/null
}

# Waits until the VM writes `file`. Fails on timeout or if the VM stops first.
wait_for_file() {
  local file=$1 timeout=$2 elapsed=0
  until [ -f "$file" ]; do
    if [ "$(docker inspect --format '{{.State.Running}}' "$CONTAINER")" != true ]; then
      echo "The VM stopped before writing $(basename "$file"). Its last log lines:" >&2
      docker logs --tail 20 "$CONTAINER" >&2 2>&1
      return 1
    fi
    if [ "$elapsed" -ge "$timeout" ]; then
      echo "The VM did not write $(basename "$file") within ${timeout}s." >&2
      return 1
    fi
    sleep 5
    elapsed=$((elapsed + 5))
  done
}

new_shared_folder() {
  local shared
  shared=$(mktemp -d "$E2E_HOME/shared.XXXXXX")
  chmod a+rwx "$shared"
  echo "$shared"
}
