@echo off
rem Runs from the Startup folder at each logon, in the desktop session. UAC is off, so it's elevated.
rem Test runs put run-tests.ps1 in the shared folder; without it, this is the image build, so it powers off.
set share=\host.lan\Data
:wait
if not exist %share%\ (
  timeout /t 2 /nobreak >nul
  goto wait
)
if exist %share%\run-tests.ps1 (
  start "" powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File %share%\run-tests.ps1
  exit /b
)
echo provisioned>%share%\provisioned
shutdown /s /t 0
