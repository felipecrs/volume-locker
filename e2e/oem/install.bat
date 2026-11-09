@echo off
rem Runs once as SYSTEM at the end of Windows setup.
schtasks /create /f /tn VolumeLockerE2E /sc onlogon /ru Docker /rl highest /it /tr "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\OEM\logon.ps1"
rem Docker may already be logged on by now, in which case the logon trigger has passed.
schtasks /run /tn VolumeLockerE2E
