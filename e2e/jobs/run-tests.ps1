$share = '\\host.lan\Data'
New-Item -ItemType Directory C:\e2e -Force | Out-Null
Copy-Item "$share\e2e.exe", "$share\VolumeLocker.exe" C:\e2e\
$env:VOLUME_LOCKER_E2E = '1'
$env:RUST_BACKTRACE = '1'
& C:\e2e\e2e.exe --test-threads=1 | Out-Default
Set-Content "$share\outbox\exit-code" $LASTEXITCODE
