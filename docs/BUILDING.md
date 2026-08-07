# Building MeetingOne from Source

This guide provides detailed instructions for building MeetingOne from source on different operating systems.

<details>
<summary>Linux</summary>

## 🐧 Building on Linux

### 1. Install Dependencies

```bash
# Ubuntu/Debian
sudo apt update
sudo apt install build-essential cmake git

# Fedora/RHEL
sudo dnf install gcc-c++ cmake git

# Arch Linux
sudo pacman -S base-devel cmake git
```

### 2. Build and Run

```bash
cd frontend
pnpm install
pnpm run tauri:dev    # development mode (hot reload)
pnpm run tauri:build  # production build
```

### Build Output Location

After a successful production build:

```
frontend/src-tauri/target/release/bundle/appimage/MeetingOne_<version>_amd64.AppImage
```

</details>

<details>
<summary>macOS</summary>

## 🍎 Building on macOS

### 1. Install Dependencies

```bash
# Install Homebrew (if not already installed)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Install required tools
brew install cmake node pnpm
```

### 2. Build and Run

```bash
cd frontend
pnpm install
pnpm run tauri:dev    # development mode (hot reload)
pnpm run tauri:build  # production build
```

</details>

<details>
<summary>Windows</summary>

## 🪟 Building on Windows

### 1. Install Dependencies

- **Node.js:** Download and install from [nodejs.org](https://nodejs.org/).
- **Rust:** Install from [rust-lang.org](https://www.rust-lang.org/tools/install).
- **Visual Studio Build Tools:** Install the "Desktop development with C++" workload from the Visual Studio Installer.
- **CMake:** Download and install from [cmake.org](https://cmake.org/download/).

### 2. Build and Run

```powershell
cd frontend
pnpm install
pnpm run tauri:dev    # development mode (hot reload)
pnpm run tauri:build  # production build
```

</details>
