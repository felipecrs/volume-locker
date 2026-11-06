# First boot: install the USB/IP driver, then power off so the next boot loads it.
# Run from a local copy: Windows shows a security prompt for programs started from a network share.
Copy-Item '\\host.lan\Data\USBip-x64.exe' $env:TEMP
$installer = Start-Process "$env:TEMP\USBip-x64.exe" -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -PassThru
if (-not $installer.WaitForExit(900000)) { throw 'the USB/IP installer timed out' }
"installer exit code: $($installer.ExitCode)"
shutdown /s /t 5
