# Runs at each logon in the desktop session. With tests in the shared folder it runs them;
# without, this is provisioning, so it reports and powers off.
$share = '\\host.lan\Data'
while (-not (Test-Path "$share\")) { Start-Sleep 2 }

if (-not (Test-Path "$share\e2e.exe")) {
    $usbip = if (Test-Path 'C:\Program Files\USBip\usbip.exe') { 'usbip installed' } else { 'usbip MISSING' }
    Set-Content "$share\provisioned" $usbip
    shutdown /s /t 0
    exit
}

Set-Content "$share\started" 'started'
New-Item -ItemType Directory C:\e2e -Force | Out-Null
Copy-Item "$share\e2e.exe", "$share\VolumeLocker.exe" C:\e2e\
$env:VOLUME_LOCKER_E2E = '1'
$env:RUST_BACKTRACE = '1'
# Output goes to a file, not a pipe, so processes the tests start can't hold it open.
cmd /d /c "C:\e2e\e2e.exe --test-threads=1 >`"$share\tests.log`" 2>&1"
Set-Content "$share\exit-code" $LASTEXITCODE
