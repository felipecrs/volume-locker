@echo off
rem Runs once as SYSTEM at the end of Windows setup, before the first logon.
copy /y C:\OEM\logon.cmd "%ProgramData%\Microsoft\Windows\Start Menu\Programs\StartUp\"
