# Copied into the VM's shared folder by run.sh, and run by the logon script.
$share = '\\host.lan\Data'
Set-Content "$share\started" 'started'
New-Item -ItemType Directory C:\e2e -Force | Out-Null
Copy-Item "$share\e2e.exe", "$share\VolumeLocker.exe" C:\e2e\
$env:VOLUME_LOCKER_E2E = '1'
$env:RUST_BACKTRACE = '1'
$testArgs = (Get-Content "$share\test-args" -ErrorAction SilentlyContinue) -join ' '
# Output goes to a file, not a pipe, so processes the tests start can't hold it open.
cmd /d /c "C:\e2e\e2e.exe $testArgs >`"$share\tests.log`" 2>&1"
Set-Content "$share\exit-code" $LASTEXITCODE
