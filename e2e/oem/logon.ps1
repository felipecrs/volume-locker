# Runs at each logon in the desktop session, from the Run key that install.bat adds. UAC is off, so it's elevated.
# Test runs put the tests in the shared folder; without them, this is the image build, so it powers off.
$share = '\\host.lan\Data'
while (-not (Test-Path "$share\")) { Start-Sleep 2 }

if (-not (Test-Path "$share\e2e.exe")) {
    Set-Content "$share\provisioned" 'provisioned'
    Stop-Computer -Force
    exit
}

Set-Content "$share\started" 'started'
# Run local copies: Windows asks before running programs from a network folder.
New-Item -ItemType Directory C:\e2e -Force | Out-Null
Copy-Item "$share\*.exe" C:\e2e\
$env:CI = 'true'
$env:RUST_BACKTRACE = '1'
$testArgs = Get-Content "$share\test-args" -TotalCount 1
# cmd writes the output straight to a file, so processes the tests start can't hold a pipe open.
cmd /d /c "C:\e2e\e2e.exe $testArgs > `"$share\tests.log`" 2>&1"
Set-Content "$share\exit-code" $LASTEXITCODE
