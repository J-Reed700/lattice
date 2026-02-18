# Build Troubleshooting Guide

This guide helps resolve common build issues for Recall Desktop.

## Quick Start

**Before building, run the setup script:**

```bash
./scripts/setup-dev-env.sh
```

This will install all required system dependencies.

---

## Common Build Errors

### 1. GTK/GDK System Library Not Found

**Error Message:**
```
The system library `gdk-3.0` required by crate `gdk-sys` was not found.
The file `gdk-3.0.pc` needs to be installed and the PKG_CONFIG_PATH environment variable must contain its parent directory.
```

**Cause:** Missing GTK3 development libraries.

**Solution (Ubuntu/Debian):**
```bash
sudo apt-get update
sudo apt-get install -y \
    libgtk-3-dev \
    libwebkit2gtk-4.1-dev \
    libjavascriptcoregtk-4.1-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    libssl-dev \
    libatk1.0-dev \
    build-essential \
    pkg-config
```

**Solution (Fedora/RHEL):**
```bash
sudo dnf install -y \
    gtk3-devel \
    webkit2gtk4.0-devel \
    libappindicator-gtk3-devel \
    librsvg2-devel \
    openssl-devel \
    atk-devel \
    pkg-config
```

**Solution (Arch Linux):**
```bash
sudo pacman -S --needed \
    gtk3 \
    webkit2gtk \
    libappindicator-gtk3 \
    librsvg \
    openssl \
    atk \
    pkg-config
```

---

### 2. Objective-C Compiler Missing (Linux)

**Error Message:**
```
cc: fatal error: cannot execute 'cc1obj': execvp: No such file or directory
```

**Cause:** Missing GCC Objective-C compiler (needed for some cross-platform dependencies).

**Solution (Ubuntu/Debian):**
```bash
sudo apt-get install -y gobjc gobjc++
```

**Solution (Fedora/RHEL):**
```bash
sudo dnf install -y gcc-objc gcc-objc++
```

**Solution (Arch Linux):**
```bash
# Usually included with gcc, verify:
pacman -Q gcc
```

---

### 3. Platform-Specific Dependency Issues

**Error Message:**
```
objc/NSObject.h: No such file or directory
```

**Cause:** Platform-specific dependencies (like `mistralrs` with `metal` feature) trying to build on wrong platform.

**Solution:**
The `Cargo.toml` now correctly separates platform-specific dependencies:
- **macOS**: Uses `metal` feature for GPU acceleration
- **Linux**: Uses CPU-only version
- **Windows**: Uses CPU-only version

No action needed if using latest `Cargo.toml`.

---

### 4. ONNX Runtime Download Failure

**Error Message:**
```
failed to download https://github.com/microsoft/onnxruntime/releases/download/...
Transport(Transport { kind: Dns, message: Some("resolve dns name 'github.com:443'")
```

**Cause:** Network connectivity issue or DNS resolution failure.

**Solutions:**

**Option 1: Pre-download ONNX Runtime** (Recommended for CI/CD)
```bash
# Set environment variable to use system ONNX Runtime
export ORT_STRATEGY=system

# Or download manually
wget https://github.com/microsoft/onnxruntime/releases/download/v1.16.0/onnxruntime-linux-x64-1.16.0.tgz
export ORT_USE_SYSTEM_LIBONNXRUNTIME=1
```

**Option 2: Check Network**
```bash
# Verify DNS resolution
ping github.com

# Check proxy settings
echo $HTTP_PROXY
echo $HTTPS_PROXY
```

**Option 3: Disable Features** (if not using embeddings)
```bash
cargo build --no-default-features
```

---

### 5. Rust Version Issues

**Error Message:**
```
error: package `<crate>` requires Rust version ...
```

**Cause:** Outdated Rust toolchain.

**Solution:**
```bash
rustup update stable
rustc --version  # Verify ≥ 1.70
```

---

### 6. Node.js/npm Not Found

**Error Message:**
```
npm: command not found
```

**Cause:** Node.js not installed.

**Solution (Ubuntu/Debian):**
```bash
curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -
sudo apt-get install -y nodejs
```

**Solution (macOS):**
```bash
brew install node
```

**Solution (Windows):**
Download from https://nodejs.org/

---

### 7. SQLx Compile-Time Verification Failures

**Error Message:**
```
error: no rows returned by query
```

**Cause:** Database schema mismatch or missing `.sqlx` offline mode.

**Solution:**
```bash
# Build with offline mode (no database needed)
cargo build --features sqlx/offline

# Or prepare SQLx data (requires running database)
cargo sqlx prepare
```

---

### 8. Out of Memory During Build

**Error Message:**
```
LLVM ERROR: out of memory
```

**Cause:** Insufficient RAM for parallel compilation.

**Solution:**
```bash
# Limit parallel jobs
cargo build -j 2

# Or build in release mode (uses less memory)
cargo build --release
```

---

## Platform-Specific Notes

### Linux

**Required System Libraries:**
- GTK 3.22+
- WebKit2GTK 4.1
- OpenSSL 1.1+
- pkg-config

**Display Server:**
- Wayland supported
- X11 supported

### macOS

**Requirements:**
- macOS 10.15+
- Xcode Command Line Tools
- Homebrew (recommended)

**Install Xcode tools:**
```bash
xcode-select --install
```

### Windows

**Requirements:**
- Visual Studio 2019+ or Build Tools
- Windows SDK 10+

**Install Build Tools:**
https://visualstudio.microsoft.com/downloads/

---

## Verification Steps

After fixing issues, verify the build works:

### 1. Clean Build
```bash
cd vault/desktop/src-tauri
cargo clean
cargo build
```

### 2. Run Tests
```bash
cargo test
```

### 3. Build Frontend
```bash
cd ..  # Back to vault/desktop
npm install
npm run build
```

### 4. Run Desktop App
```bash
npm run tauri:dev
```

---

## Getting Help

If you still encounter issues:

1. **Check logs:**
   ```bash
   RUST_BACKTRACE=full cargo build
   ```

2. **Clean state:**
   ```bash
   cargo clean
   rm -rf target/
   rm -rf node_modules/
   npm install
   cargo build
   ```

3. **Report issue:**
   - Include full error output
   - Include `rustc --version`
   - Include `npm --version`
   - Include OS version
   - Include output of `setup-dev-env.sh`

---

## CI/CD Notes

For automated builds:

### GitHub Actions (Ubuntu)

```yaml
- name: Install dependencies
  run: |
    sudo apt-get update
    sudo apt-get install -y \
      libgtk-3-dev \
      libwebkit2gtk-4.1-dev \
      libjavascriptcoregtk-4.1-dev \
      libayatana-appindicator3-dev \
      librsvg2-dev \
      libssl-dev \
      pkg-config

- name: Build
  run: |
    cd vault/desktop
    npm install
    cd src-tauri
    cargo build --release
```

### Docker Builds

See `vault/desktop/Dockerfile` for containerized builds.

---

## Summary of Key Dependencies

| Dependency | Ubuntu Package | Fedora Package | macOS | Windows |
|------------|---------------|----------------|-------|---------|
| GTK 3 | libgtk-3-dev | gtk3-devel | Native | Native |
| WebKit2GTK | libwebkit2gtk-4.1-dev | webkit2gtk4.0-devel | Native | N/A |
| Rust | curl + rustup | curl + rustup | brew install rust | rustup-init.exe |
| Node.js | nodejs (NodeSource) | nodejs | brew install node | nodejs.org |
| pkg-config | pkg-config | pkg-config | Built-in | N/A |
| OpenSSL | libssl-dev | openssl-devel | Built-in | Bundled |

---

**Last Updated:** 2025-11-20
