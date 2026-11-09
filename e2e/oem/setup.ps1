# Prepares Windows for the end-to-end tests: in the VM, as SYSTEM at the end of setup, and in CI.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

# Windows Server, which CI runs on, has Windows Audio turned off.
Set-Service audiosrv -StartupType Automatic
Start-Service audiosrv

$installer = Join-Path $env:TEMP 'USBip-x64.exe'
Invoke-WebRequest -UseBasicParsing -OutFile $installer `
    https://github.com/vadimgrn/usbip-win2/releases/download/v.0.9.8.1/USBip-0.9.8.1-x64.exe
if ((Get-FileHash $installer -Algorithm SHA256).Hash -ne '38CAD6D4432B52D5BB9409D9AD03B72FDFFC4ADA4CD3A48FBECA1A2752A8518A') {
    throw 'The usbip-win2 installer has an unexpected hash.'
}
Start-Process $installer -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait
if (-not (Test-Path 'C:\Program Files\USBip\usbip.exe')) { throw 'usbip-win2 did not install.' }
