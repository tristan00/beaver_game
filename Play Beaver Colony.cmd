@echo off
setlocal
cd /d "%~dp0"
set "BC_WATER_DETAILED="
set "BC_SPEED="
set "BC_PERF_EXIT="
set "BC_WAIT_DAYS="
set "BC_HUD_DEMO="
set "BC_HUD_DAYS="
set "BC_SETTLE="
set "BC_PERF="
set "BC_MENU="
if not exist "target\release\beaver_colony.exe" (
  echo Build the game first: cargo build --offline --release -p game
  pause
  exit /b 1
)
start "" "%~dp0target\release\beaver_colony.exe" %*
