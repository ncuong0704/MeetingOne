@echo off
set PATH=C:\Users\HP\.cargo\bin;C:\Program Files\LLVM\bin;C:\Program Files\CMake\bin;%PATH%
set LIBCLANG_PATH=C:\Program Files\LLVM\bin
echo Rust version:
rustc --version
echo Cargo version:
cargo --version
echo.
echo Starting Tauri dev (CPU mode)...
pnpm run tauri:dev:cpu
