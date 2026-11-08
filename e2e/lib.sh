# Shared by provision.sh and run.sh.
E2E_HOME=${E2E_HOME:-$HOME/.cache/volume-locker-e2e}
E2E_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
STORAGE=$E2E_HOME/windows
# The ISO file, ISO URL or dockur version the image was built from. Every boot must pass
# the same one, or dockur treats the image as outdated and installs Windows again.
SOURCE_FILE=$E2E_HOME/windows-source
CONTAINER=volume-locker-e2e
IMAGE=docker.io/dockurr/windows@sha256:16fd7c39a3e48bb946d29c59dc6015942e5b900a1fa5d7e5552053bfc48ee2ea

# Creates and boots the VM. Extra arguments go to `docker run`.
boot_vm() {
  local shared=$1 windows media
  shift
  windows=$(cat "$SOURCE_FILE")
  if [ -f "$windows" ]; then
    media=(--volume "$windows:/boot.iso:ro")
  else
    media=(--env VERSION="$windows")
  fi
  rm -f "$shared/agent.ready"
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
  docker run --detach --name "$CONTAINER" \
    --env RAM_SIZE=4G --env CPU_CORES=4 --env DISK_SIZE=32G \
    --publish 127.0.0.1:8006:8006 \
    --device /dev/kvm --device /dev/net/tun --cap-add NET_ADMIN \
    --volume "$STORAGE:/storage" --volume "$shared:/shared" \
    --stop-timeout 120 "${media[@]}" "$@" "$IMAGE" >/dev/null
}

# Waits until the guest agent reports in. Fails on timeout or if the VM stops.
wait_for_agent() {
  local shared=$1 timeout=$2 elapsed=0
  until [ -f "$shared/agent.ready" ]; do
    if [ "$(docker inspect --format '{{.State.Running}}' "$CONTAINER")" != true ]; then
      echo "The VM stopped before its agent started. Its last log lines:" >&2
      docker logs --tail 20 "$CONTAINER" >&2 2>&1
      return 1
    fi
    if [ "$elapsed" -ge "$timeout" ]; then
      echo "The agent did not start within ${timeout}s." >&2
      return 1
    fi
    sleep 5
    elapsed=$((elapsed + 5))
  done
}

# Runs jobs/<name>.ps1 in the VM's desktop session and prints what it wrote.
run_job() {
  local shared=$1 name=$2 timeout=$3 elapsed=0 status=0
  install -m 666 "$E2E_DIR/jobs/$name.ps1" "$shared/inbox/$name.ps1"
  until [ -f "$shared/outbox/$name.out.done" ]; do
    if [ "$elapsed" -ge "$timeout" ]; then
      echo "Job $name did not finish within ${timeout}s." >&2
      status=1
      break
    fi
    sleep 2
    elapsed=$((elapsed + 2))
  done
  sed 's/\r$//' "$shared/outbox/$name.out" 2>/dev/null || true
  return "$status"
}

new_shared_folder() {
  local shared
  shared=$(mktemp -d "$E2E_HOME/shared.XXXXXX")
  mkdir "$shared/inbox" "$shared/outbox"
  chmod -R a+rwX "$shared"
  echo "$shared"
}
