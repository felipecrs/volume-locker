@echo off
rem Runs from the Startup folder at each logon, in the desktop session. UAC is off, so it's elevated.
rem Test runs put the tests in the shared folder; without them, this is the image build, so it powers off.
set share=\\host.lan\Data
:wait
if not exist %share%\ (
  timeout /t 2 /nobreak >nul
  goto wait
)
if not exist %share%\e2e.exe (
  echo provisioned>%share%\provisioned
  shutdown /s /t 0
  exit /b
)
echo started>%share%\started
rem Run local copies: Windows asks before running programs from a network folder.
mkdir C:\e2e 2>nul
copy /y %share%\e2e.exe C:\e2e\ >nul
copy /y %share%\VolumeLocker.exe C:\e2e\ >nul
set CI=true
set RUST_BACKTRACE=1
set args=
set /p args=<%share%\test-args
rem Output goes to a file, not a pipe, so processes the tests start can't hold it open.
C:\e2e\e2e.exe %args% >%share%\tests.log 2>&1
rem The redirection goes first, since in "echo 1>file" cmd reads "1>" as a handle.
>%share%\exit-code echo %errorlevel%
