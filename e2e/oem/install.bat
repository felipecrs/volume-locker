@echo off
rem Dockur runs this once, as SYSTEM, at the end of Windows setup. It only runs a file with this name.
rem From then on, Windows runs logon.ps1 at every logon.
reg add HKLM\Software\Microsoft\Windows\CurrentVersion\Run /v VolumeLockerE2E /t REG_SZ /f /d "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\OEM\logon.ps1"
