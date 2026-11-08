#!/usr/bin/env bash
# Builds the Windows base image that run.sh boots from, replacing any existing one. Needs Docker and KVM.
# Usage: provision.sh [ISO file, ISO URL or dockur Windows version]
set -euo pipefail
source "$(dirname "$0")/lib.sh"

WINDOWS=${1:-11l}
USBIP_URL=https://github.com/vadimgrn/usbip-win2/releases/download/v.0.9.8.1/USBip-0.9.8.1-x64.exe
USBIP_SHA256=38cad6d4432b52d5bb9409d9ad03b72fdffc4ada4cd3a48fbeca1a2752a8518a
INSTALL_TIMEOUT=7200
JOB_TIMEOUT=1200
SHUTDOWN_TIMEOUT=300

wait_for_power_off() {
  timeout "$SHUTDOWN_TIMEOUT" docker wait "$CONTAINER" >/dev/null
}

mkdir -p "$E2E_HOME"
rm -rf "$STORAGE"
mkdir -p "$STORAGE"
shared=$(new_shared_folder)
trap 'docker rm -f "$CONTAINER" >/dev/null 2>&1 || true; rm -rf "$shared"' EXIT

curl --fail --location --silent --show-error --output "$shared/USBip-x64.exe" "$USBIP_URL"
echo "$USBIP_SHA256  $shared/USBip-x64.exe" | sha256sum --check --quiet
chmod a+rx "$shared/USBip-x64.exe"

[ -f "$WINDOWS" ] && WINDOWS=$(realpath "$WINDOWS")
echo "$WINDOWS" >"$SOURCE_FILE"

echo "Installing Windows. Watch it at http://localhost:8006"
boot_vm "$shared" --volume "$E2E_DIR/oem:/oem:ro"
wait_for_agent "$shared" "$INSTALL_TIMEOUT"

echo "Installing the USB/IP driver."
run_job "$shared" install-usbip "$JOB_TIMEOUT"
wait_for_power_off
echo "The image is ready in $STORAGE"
