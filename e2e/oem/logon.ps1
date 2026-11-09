# Runs at each logon in the desktop session. Test runs put run-tests.ps1 in the shared folder;
# without it, this is provisioning, so it reports and powers off.
$share = '\\host.lan\Data'
while (-not (Test-Path "$share\")) { Start-Sleep 2 }

if (Test-Path "$share\run-tests.ps1") {
    & "$share\run-tests.ps1"
    exit
}

Set-Content "$share\provisioned" 'provisioned'
shutdown /s /t 0
