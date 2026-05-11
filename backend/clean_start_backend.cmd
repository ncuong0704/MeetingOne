@echo off
setlocal enabledelayedexpansion

set "PORT=5167"

echo === Environment Check ===
echo.

if not exist "app" (
    echo Python backend directory not found. Please check your installation
    goto :eof
)

if not exist "app\main.py" (
    echo Python backend main.py not found. Please check your installation
    goto :eof
)

if not exist "venv" (
    echo Virtual environment not found.
    echo Run: python -m venv venv ^&^& venv\Scripts\activate ^&^& pip install -r requirements.txt
    goto :eof
)

echo === Backend App Check ===
echo.

echo Checking for processes on port %PORT%...
set "PORT_IN_USE="
for /f "tokens=5" %%a in ('netstat -ano ^| findstr ":%PORT%.*LISTENING"') do (
    set "PORT_IN_USE=%%a"
)

if defined PORT_IN_USE (
    echo Backend app is running on port %PORT%
    set /p REPLY="Kill it? (y/N) "
    if /i not "!REPLY!"=="y" (
        echo User chose not to terminate existing backend app
        goto :eof
    )
    taskkill /F /PID !PORT_IN_USE! 2>nul
    timeout /t 1 >nul
)

echo === Starting Python Backend ===
echo.

echo Activating virtual environment...
call venv\Scripts\activate.bat
if %ERRORLEVEL% neq 0 (
    echo Failed to activate virtual environment
    goto :eof
)

pip show fastapi >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo FastAPI not found. Run: pip install -r requirements.txt
    goto :eof
)

echo Running Python backend on port %PORT%...
start "Python Backend" cmd /k "call venv\Scripts\activate.bat && python app\main.py"

timeout /t 5 >nul

for /f "tokens=2" %%a in ('tasklist /fi "imagename eq python.exe" /fo list ^| findstr "PID:"') do (
    set "PYTHON_PID=%%a"
)

echo.
echo === Python Backend Started ===
echo Port: %PORT%
echo.
echo Press any key to exit (backend continues running in its own window)
pause >nul

goto :eof
