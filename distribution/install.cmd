@echo off
REM Bridge for double-click and cmd.exe users: hands off to the PowerShell
REM installer with the documented execution-policy bypass for this file only.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
