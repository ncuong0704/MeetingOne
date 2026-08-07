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
