#!/usr/bin/env bash
# Builds the Windows base image that run.sh boots from, replacing any existing one. Needs Docker and KVM.
set -euo pipefail
source "$(dirname "$0")/lib.sh"

USBIP_URL=https://github.com/vadimgrn/usbip-win2/releases/download/v.0.9.8.1/USBip-0.9.8.1-x64.exe
USBIP_SHA256=38cad6d4432b52d5bb9409d9ad03b72fdffc4ada4cd3a48fbeca1a2752a8518a
INSTALL_TIMEOUT=7200

mkdir -p "$E2E_HOME"
rm -rf "$STORAGE"
mkdir -p "$STORAGE"
shared=$(new_shared_folder)
oem=$(mktemp -d "$E2E_HOME/oem.XXXXXX")
trap 'docker rm -f "$CONTAINER" >/dev/null 2>&1 || true; rm -rf "$shared" "$oem"' EXIT

# Dockur copies these to C:\OEM and runs install.bat at the end of setup.
cp "$E2E_DIR"/oem/* "$oem/"
curl --fail --location --silent --show-error --output "$oem/USBip-x64.exe" "$USBIP_URL"
echo "$USBIP_SHA256  $oem/USBip-x64.exe" | sha256sum --check --quiet
chmod -R a+rX "$oem"

echo "Installing Windows. Watch it at http://localhost:8006"
boot_vm "$shared" --volume "$oem:/oem:ro"
# logon.ps1 writes this and powers off at the first logon, since no tests are in the shared folder.
wait_for_file "$shared/provisioned" "$INSTALL_TIMEOUT"
timeout 300 docker wait "$CONTAINER" >/dev/null
cat "$shared/provisioned"
grep -q '^usbip installed' "$shared/provisioned"
echo "The image is ready in $STORAGE"
