@echo off
rem Compila PdfTool. Metti questo file in AmbienteRust e il progetto in Progetti\PdfTool.
title Costruisci PdfTool
set "ROOT=%~dp0"
if "%ROOT:~-1%"=="\" set "ROOT=%ROOT:~0,-1%"
set "CARGO_HOME=%ROOT%\rust\cargo"
set "RUSTUP_HOME=%ROOT%\rust\rustup"
set "PATH=%ROOT%\winlibs\mingw64\bin;%ROOT%\rust\cargo\bin;%ROOT%\git\cmd;%ROOT%\git\bin;%PATH%"
set "PROJ=%ROOT%\Progetti\PdfTool"
if not exist "%PROJ%\Cargo.toml" (
  echo ERRORE: non trovo %PROJ%\Cargo.toml
  pause
  exit /b 1
)
taskkill /f /im pdftool.exe >nul 2>&1
cd /d "%PROJ%"
echo === Compilo (la prima volta ci vogliono diversi minuti) ===
cargo build --release
if errorlevel 1 goto fail
set "OUT=%PROJ%\dist\PdfTool-portable"
if exist "%OUT%" rmdir /s /q "%OUT%"
mkdir "%OUT%"
copy /y "target\release\pdftool.exe" "%OUT%\PdfTool.exe" >nul
if errorlevel 1 goto fail
echo.
echo FATTO: %OUT%\PdfTool.exe
pause
exit /b 0
:fail
echo.
echo ERRORE: compilazione non riuscita. Controlla i messaggi sopra.
pause
exit /b 1
