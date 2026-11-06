# Second boot: check the driver is there and the evaluation licence is active, then power off.
if (-not (Test-Path 'C:\Program Files\USBip\usbip.exe')) { throw 'USB/IP is not installed' }
cscript //nologo C:\Windows\System32\slmgr.vbs /ato
cscript //nologo C:\Windows\System32\slmgr.vbs /xpr
shutdown /s /t 5
