@echo off
rem Runs once at the end of Windows setup. The task only fires on later logons, so run it now too.
schtasks /create /f /tn VolumeLockerE2EAgent /sc onlogon /ru Docker /rl highest /it /tr "powershell.exe -NoProfile -STA -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\OEM\agent.ps1"
schtasks /run /tn VolumeLockerE2EAgent
