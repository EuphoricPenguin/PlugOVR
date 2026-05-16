@echo off

echo Deleting old VST3 file...
del /f "target\release\plugovr.vst3"

echo Building release version...
cargo build --release
if %ERRORLEVEL% NEQ 0 (
    echo Build failed!
    pause
    exit /b %ERRORLEVEL%
)

echo Renaming plugovr.dll to plugovr.vst3...
move "target\release\plugovr.dll" "target\release\plugovr.vst3"

echo Copying .voice files alongside the VST3...
if exist "bin\compiled_voices\*.voice" (
    copy "bin\compiled_voices\*.voice" "target\release\" >nul
    echo   Copied .voice files from bin\compiled_voices\
) else (
    echo   No .voice files found in bin\compiled_voices\
)

echo Done!
pause