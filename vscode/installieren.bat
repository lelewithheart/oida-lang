@echo off
rem Baut die VS-Code-Erweiterung und installiert sie gleich in VS Code.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0bauen.ps1" -Installieren
echo.
pause
