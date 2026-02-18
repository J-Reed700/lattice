# Quick Setup Reference

**Get building in 5 minutes**

---

## TL;DR

### Linux/macOS
```bash
cd vault/desktop
./scripts/setup-dev-env.sh
npm install
cd src-tauri && cargo build
```

### Windows
```powershell
# 1. Install Visual Studio Build Tools
# https://visualstudio.microsoft.com/downloads/

# 2. Install Rust
# https://rustup.rs/

# 3. Install Node.js
# https://nodejs.org/

# 4. Build
cd vault\desktop
npm install
cd src-tauri
cargo build
```

---

## Common Issues

### "gdk-3.0 not found"
Run setup script:
```bash
./scripts/setup-dev-env.sh
```

Or install manually:
```bash
sudo apt-get install libgtk-3-dev libwebkit2gtk-4.1-dev pkg-config
```

### "cannot execute 'cc1obj'"
Install Objective-C compiler:
```bash
sudo apt-get install gobjc gobjc++
```

### "ONNX Runtime download failed"
Check network connection:
```bash
ping github.com
```

Or use system ONNX:
```bash
export ORT_STRATEGY=system
```

---

## Build Modes

### Development
```bash
cargo build                    # Debug build
npm run tauri:dev             # Hot-reload dev server
```

### Production
```bash
cargo build --release         # Optimized build
npm run build:production      # Full production build
```

### Testing
```bash
cargo test                    # Rust tests
npm test                      # TypeScript tests
npm run test:e2e             # End-to-end tests
```

---

## Documentation

- **Troubleshooting:** [BUILD_TROUBLESHOOTING.md](./BUILD_TROUBLESHOOTING.md)
- **Architecture:** [ARCHITECTURE.md](./ARCHITECTURE.md)
- **Developer Guide:** [DEVELOPER_GUIDE.md](./DEVELOPER_GUIDE.md)

---

## System Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| Rust | 1.70+ | Latest stable |
| Node.js | v18 | v20 LTS |
| RAM | 4 GB | 8 GB+ |
| Disk | 2 GB | 5 GB+ |

---

## Platform-Specific

### Ubuntu/Debian
```bash
./scripts/setup-dev-env.sh
```

### Fedora/RHEL
```bash
sudo dnf install gtk3-devel webkit2gtk4.0-devel pkg-config
```

### Arch Linux
```bash
sudo pacman -S gtk3 webkit2gtk pkg-config
```

### macOS
```bash
xcode-select --install
brew install node
```

### Windows
Install:
1. Visual Studio Build Tools
2. Rust (rustup-init.exe)
3. Node.js LTS

---

**Need help?** See [BUILD_TROUBLESHOOTING.md](./BUILD_TROUBLESHOOTING.md)
