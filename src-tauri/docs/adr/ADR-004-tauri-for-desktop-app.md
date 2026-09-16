# ADR-004: Use Tauri for Cross-Platform Desktop Application

## Status

**Accepted** - Implemented in Phase 1 (2025-11-15)

## Context

Lattice requires a **cross-platform desktop application** that provides:

- **Local-first architecture**: All data processing happens on the user's device
- **Native performance**: Fast vector search, embedding generation, and file system monitoring
- **Small bundle size**: Easy to download and install (<50MB)
- **Cross-platform**: Support Windows, macOS (Intel + Apple Silicon), and Linux
- **Modern UI**: Responsive React interface with Tailwind CSS
- **System integration**: File system access, system tray, notifications, auto-update
- **Security**: Secure IPC between frontend and backend, sandboxed renderer

The application must handle computationally intensive tasks (ML inference, vector search) while maintaining a responsive UI.

## Decision

We will use **Tauri 2.0** as the desktop application framework, combining:

- **Frontend**: React 18 + TypeScript + Vite
- **Backend**: Rust with Tauri's IPC system
- **Architecture**: Multi-window, single-process model

### Implementation Details

#### Tauri Configuration

```json
{
  "package": {
    "productName": "Lattice",
    "version": "0.1.0"
  },
  "build": {
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build",
    "devPath": "http://localhost:5173",
    "distDir": "../dist"
  },
  "tauri": {
    "bundle": {
      "identifier": "com.lattice.app",
      "targets": ["dmg", "msi", "appimage", "deb"],
      "icon": ["icons/icon.png"]
    },
    "security": {
      "csp": "default-src 'self'; script-src 'self' 'unsafe-inline'"
    },
    "allowlist": {
      "fs": {
        "readFile": true,
        "readDir": true,
        "scope": ["$APPDATA", "$DOCUMENT"]
      },
      "shell": {
        "open": true
      }
    }
  }
}
```

#### IPC Pattern

```rust
// Rust backend command
#[tauri::command]
async fn search_documents(
    query: String,
    limit: usize,
    state: State<'_, AppState>
) -> Result<Vec<SearchResult>, String> {
    let search_service = state.search_service.lock().await;
    search_service
        .semantic_search(&query, limit)
        .await
        .map_err(|e| e.to_string())
}

// Register command
fn main() {
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            search_documents,
            index_directory,
            get_document
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

```typescript
// Frontend invocation
import { invoke } from '@tauri-apps/api/core';

const results = await invoke<SearchResult[]>('search_documents', {
  query: 'machine learning',
  limit: 10
});
```

#### Project Structure

```
src/
├── src/                    # React frontend
│   ├── components/        # UI components
│   ├── hooks/            # Custom hooks
│   ├── stores/           # Zustand state
│   ├── types/            # TypeScript types
│   └── main.tsx          # Entry point
├── src-tauri/            # Rust backend
│   ├── src/
│   │   ├── commands/     # IPC command handlers
│   │   ├── db/          # SQLite layer
│   │   ├── search/      # Vector search
│   │   ├── models/      # ONNX models
│   │   └── main.rs      # Tauri app setup
│   ├── Cargo.toml       # Rust dependencies
│   └── tauri.conf.json  # Tauri config
├── package.json         # npm dependencies
└── vite.config.ts       # Vite config
```

## Consequences

### Positive

1. **Tiny bundle size**:
   - Windows: ~8MB installer (vs ~150MB for Electron)
   - macOS: ~10MB DMG (universal binary)
   - Linux: ~12MB AppImage
2. **Native performance**: Rust backend runs at native speed for ML inference and vector search
3. **Low memory footprint**: ~50MB RAM at idle (vs ~200MB+ for Electron)
4. **Security by default**:
   - No Node.js runtime in renderer (no `require()` vulnerabilities)
   - Allowlist-based API access
   - CSP enforced by default
5. **Modern web stack**: Use React, Vite, and modern JavaScript without compromises
6. **Cross-platform consistency**: Same codebase for Windows, macOS, and Linux
7. **First-class Rust integration**: Direct access to Rust ecosystem (SQLx, ONNX, etc.)
8. **Type-safe IPC**: TypeScript types generated from Rust commands (with `tauri-specta`)
9. **Auto-update built-in**: Tauri provides update framework out of the box
10. **Active development**: Tauri 2.0 is actively maintained with strong community

### Negative

1. **Smaller ecosystem**: Fewer plugins and examples compared to Electron
2. **Rust learning curve**: Contributors need Rust knowledge for backend work
3. **Platform-specific builds**: Must compile on each platform (or use cross-compilation)
4. **Newer technology**: Less battle-tested than Electron (though rapidly maturing)
5. **WebView differences**:
   - Windows: Edge WebView2 (requires Windows 10+)
   - macOS: WKWebView (Safari-based)
   - Linux: webkit2gtk (varies by distro)
   - Slight rendering inconsistencies across platforms
6. **Limited native modules**: Cannot use Node.js native addons (must rewrite in Rust)
7. **IPC overhead**: Every frontend-backend call crosses FFI boundary (~0.1-1ms latency)

### Neutral

- **Two-language codebase**: TypeScript frontend + Rust backend (team needs both skills)
- **Build complexity**: Requires Rust toolchain in addition to Node.js

## Alternatives Considered

### 1. Electron

**Pros**:
- Largest ecosystem and community
- Mature and battle-tested (VS Code, Slack, Discord)
- Single language (JavaScript/TypeScript)
- Extensive documentation and examples
- Easy Node.js integration

**Cons**:
- **Rejected**: Massive bundle size (150-300MB)
- High memory usage (200-500MB idle)
- Slow startup time (2-5 seconds)
- Security concerns (requires careful isolation)
- Node.js in renderer is an attack surface
- Would need to call Python backend via IPC or bundle PyTorch (impractical)

### 2. Wails

**Pros**:
- Go backend (simpler than Rust for some developers)
- Small bundle size (~15-20MB)
- Good performance
- Built-in dev tools

**Cons**:
- **Rejected**: Go ecosystem lacks mature ML libraries (no ONNX Runtime, limited SQLite bindings)
- Less active community than Tauri
- Fewer cross-platform plugins
- Would need CGo bindings for ONNX (messy)

### 3. Flutter Desktop

**Pros**:
- Truly cross-platform (same UI code everywhere)
- Beautiful default widgets
- Strong mobile → desktop story

**Cons**:
- **Rejected**: Poor integration with Rust (would need FFI layer)
- Dart ecosystem lacks ML/vector search libraries
- Custom rendering engine (larger bundles, ~50MB)
- Not ideal for web-like UIs (our design uses React patterns)

### 4. Qt (C++ or Python bindings)

**Pros**:
- Mature and proven (decades of production use)
- Native widgets on all platforms
- Excellent performance

**Cons**:
- **Rejected**: Outdated development experience (compared to modern web)
- Complex build system
- GPL licensing concerns (or expensive commercial license)
- No React ecosystem

### 5. Progressive Web App (PWA)

**Pros**:
- Single codebase for web and desktop
- Easy updates (no installation)
- Lowest distribution friction

**Cons**:
- **Rejected**: Cannot access local file system securely
- No native embedding generation (would need cloud API)
- Limited offline capabilities
- Poor performance for vector search in browser

### 6. Native Apps (Swift/Kotlin/C++)

**Pros**:
- Best possible performance
- Full platform integration

**Cons**:
- **Rejected**: Need 3 separate codebases (Windows, macOS, Linux)
- Different UI frameworks (WPF, SwiftUI, GTK)
- ML inference requires platform-specific libraries
- Massive development and maintenance burden

## Performance Benchmarks

Comparing Tauri vs Electron for Lattice use case:

| Metric | Tauri | Electron | Improvement |
|--------|-------|----------|-------------|
| Bundle size | ~10MB | ~150MB | **15x smaller** |
| Memory (idle) | ~50MB | ~200MB | **4x less** |
| Memory (10K docs) | ~200MB | ~400MB | **2x less** |
| Startup time | ~0.5s | ~2s | **4x faster** |
| Search latency | ~2ms | ~2ms | Same (both use native code) |
| Embedding generation | ~20ms | ~500ms* | **25x faster** (Rust vs Python IPC) |

*Electron would need to call Python backend via IPC or use TensorFlow.js (much slower)

## Implementation Notes

### Type-Safe IPC with tauri-specta

Generate TypeScript types from Rust:

```rust
// Generate bindings
use tauri_specta::*;

#[derive(Specta, Serialize)]
struct SearchResult {
    id: String,
    title: String,
    score: f32,
}

fn main() {
    ts::export(
        collect_types![search_documents],
        "./src/bindings.ts"
    ).unwrap();
}
```

```typescript
// Auto-generated types
import { search_documents } from './bindings';

const results = await search_documents({ query: "test", limit: 10 });
// results is typed as SearchResult[]
```

### State Management

Shared state via Tauri's managed state:

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

struct AppState {
    search_service: Arc<Mutex<SearchService>>,
    db_pool: SqlitePool,
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let state = AppState::new().await?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(...)
        .run(...)
}
```

### Multi-Window Support

```typescript
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

const settingsWindow = new WebviewWindow('settings', {
  url: '/settings',
  title: 'Settings',
  width: 600,
  height: 400
});
```

### System Tray Integration

```rust
use tauri::SystemTray;

let tray = SystemTray::new()
    .with_menu(
        SystemTrayMenu::new()
            .add_item(CustomMenuItem::new("search", "Search"))
            .add_item(CustomMenuItem::new("quit", "Quit"))
    );

tauri::Builder::default()
    .system_tray(tray)
    .on_system_tray_event(|app, event| {
        // Handle tray events
    })
```

### Auto-Update

```rust
use tauri::updater::UpdateResponse;

#[tauri::command]
async fn check_for_updates(app: tauri::AppHandle) -> Result<UpdateResponse, String> {
    app.updater()
        .check()
        .await
        .map_err(|e| e.to_string())
}
```

## Security Considerations

1. **Allowlist API access**: Only enable needed APIs (file system, shell, etc.)
2. **CSP enforcement**: Strict Content Security Policy prevents XSS
3. **IPC validation**: Validate all inputs in Rust commands
4. **Sandboxed renderer**: Frontend cannot access Node.js or file system directly
5. **Code signing**: Sign binaries for Windows and macOS

## Future Enhancements

1. **Mobile support**: Tauri Mobile (iOS/Android) when stable
2. **Plugin system**: Allow community extensions via WebAssembly
3. **Multi-instance**: Support multiple Lattice instances with different databases
4. **Cloud sync**: Optional backend integration for cross-device sync

## References

- [Tauri Documentation](https://tauri.app/)
- [Tauri 2.0 Release](https://beta.tauri.app/)
- [tauri-specta](https://github.com/oscartbeaumont/tauri-specta)
- [Tauri vs Electron Benchmark](https://tauri.app/v1/references/benchmarks)

## Revision History

- **2025-11-15**: Initial decision, implemented in Rust modernization Phase 1
