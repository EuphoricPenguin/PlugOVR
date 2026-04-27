@echo off

echo Deleting old VST3 file...
del /f "target\release\plugovr.vst3"

echo Building release version...
cargo build --release
pause;
echo Renaming plugovr.dll to plugovr.vst3...
move "target\release\plugovr.dll" "target\release\plugovr.vst3"

echo Done!
