#!/usr/bin/env bash
# Builds the Windows base image that run.sh boots from, replacing any existing one. Needs Docker and KVM.
set -euo pipefail
source "$(dirname "$0")/lib.sh"

INSTALL_TIMEOUT=7200

mkdir -p "$E2E_HOME"
rm -rf "$STORAGE"
mkdir -p "$STORAGE"
shared=$(new_shared_folder)
trap 'docker rm -f "$CONTAINER" >/dev/null 2>&1 || true; rm -rf "$shared"' EXIT

echo "Installing Windows. Watch it at http://localhost:8006"
# Dockur copies oem/ to C:\OEM and runs install.bat at the end of setup.
boot_vm "$shared" --volume "$E2E_DIR/oem:/oem:ro"
# logon.ps1 writes this and powers off at the first logon, since no tests are in the shared folder.
wait_for_file "$shared/provisioned" "$INSTALL_TIMEOUT"
timeout 300 docker wait "$CONTAINER" >/dev/null
cat "$shared/provisioned"
grep -q '^usbip installed' "$shared/provisioned"
echo "The image is ready in $STORAGE"
