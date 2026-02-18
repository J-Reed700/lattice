# Vault Desktop

**Your local-first universal memory search application**

Vault Desktop is a Tauri-based desktop application that indexes and searches across all your files using semantic vector search. Built with Rust + React, it provides fast, privacy-focused search for documents, images, audio, and more.

## 🎯 Features

- **Universal Search** - Find anything across all your files
- **Semantic Understanding** - Search by meaning, not just keywords
- **Multi-Modal** - Search images, documents, audio, video
- **Local-First** - All data stays on your device
- **Fast** - Native Rust performance with SQLite
- **Privacy-Focused** - Zero telemetry, zero cloud dependencies

## 🏗️ Project Status

**Foundation: ✅ Complete**

The Tauri project structure is fully initialized with:
- ✅ Database layer (SQLite with full schema)
- ✅ Service architecture (embedding, file watching, sync)
- ✅ IPC command handlers
- ✅ Frontend skeleton (React + TypeScript + Vite)
- ✅ Complete documentation

**Next Steps: 🚧 Implementation**

See [PROJECT_STATUS.md](./PROJECT_STATUS.md) for detailed status and roadmap.

**Frontend Design System: ⚠️ 25% Complete**

The frontend is being migrated from hardcoded Tailwind colors to CSS variables for consistent theming:
- ✅ Phase 1a: Dark mode variant removal (100% - 112 files cleaned)
- ✅ Phase 1b: Core component migration (5 components migrated)
- ⚠️ Phase 2-4: Remaining work (80+ components, 48-66 hours estimated)

See [DESIGN_SYSTEM_STATUS.md](./DESIGN_SYSTEM_STATUS.md) for complete status.

## 📚 Documentation

### For Users
- **[Quick Start Guide](QUICK_START.md)** - Get started in 5 minutes
- **[User Guide](USER_GUIDE.md)** - Complete feature documentation
- **[Troubleshooting](TROUBLESHOOTING.md)** - Common issues and solutions

### For Developers
- **[SETUP.md](./SETUP.md)** - Installation and setup instructions
- **[ARCHITECTURE.md](./ARCHITECTURE.md)** - System design and architecture
- **[BUILD_GUIDE.md](./BUILD_GUIDE.md)** - How to build from source
- **[PROJECT_STATUS.md](./PROJECT_STATUS.md)** - Current status and roadmap
- **[VERIFY.md](./VERIFY.md)** - Verification checklist

### Frontend Design System
- **[DESIGN_SYSTEM_STATUS.md](./DESIGN_SYSTEM_STATUS.md)** - Design system migration status (single source of truth)
- **[DESIGN_TOKENS.md](./DESIGN_TOKENS.md)** - CSS variable mapping guide
- **[REMAINING_WORK.md](./REMAINING_WORK.md)** - Detailed roadmap for Phases 2-4
- **[IMPLEMENTATION_SUMMARY.md](./IMPLEMENTATION_SUMMARY.md)** - Phase 1 implementation details
- **[MIGRATION_REPORT.md](./MIGRATION_REPORT.md)** - Dark mode variant removal report

## 🛠️ Technology Stack

### Frontend
- **React 18** - UI framework
- **TypeScript** - Type safety
- **Vite** - Build tool
- **Tailwind CSS** - Styling
- **@tauri-apps/api** - IPC communication

### Backend
- **Tauri 2.0** - Desktop framework
- **Rust** - Systems language
- **SQLite** - Local database (via sqlx)
- **ONNX Runtime** - ML inference
- **tokio** - Async runtime
- **notify** - File system watcher

### Machine Learning
- **all-MiniLM-L6-v2** - Text embeddings (384-dim)
- **CLIP** - Image embeddings (512-dim)
- **Whisper** - Audio transcription (planned)

## 🚀 Quick Start

### Prerequisites

1. **Install Rust** (required for Tauri)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Install Node.js** (v18+ recommended)
   - Download from https://nodejs.org/

3. **Install Build Tools**
   - Windows: Visual C++ Build Tools
   - macOS: Xcode Command Line Tools
   - Linux: build-essential

See [SETUP.md](./SETUP.md) for detailed instructions.

### Install Dependencies

```bash
cd C:\Code\Recall\vault\desktop
npm install
```

### Run Development Server

```bash
npm run tauri:dev
```

This will:
1. Start Vite dev server (port 5173)
2. Compile Rust backend
3. Open desktop window
4. Initialize SQLite database

### Verify Installation

Follow the steps in [VERIFY.md](./VERIFY.md) to ensure everything is set up correctly.

## 📁 Project Structure

```
vault/desktop/
├── src/                    # React frontend
│   ├── App.tsx
│   ├── components/
│   └── lib/
│
├── src-tauri/             # Rust backend
│   ├── src/
│   │   ├── main.rs        # Entry point
│   │   ├── commands/      # IPC handlers
│   │   ├── services/      # Business logic
│   │   ├── db/            # Database layer
│   │   ├── models/        # Data structures
│   │   └── utils/         # Utilities
│   ├── Cargo.toml
│   └── tauri.conf.json
│
├── package.json
├── vite.config.ts
└── Documentation files
```

## 🗄️ Database Schema

**Core Tables:**
- `documents` - File metadata and status
- `text_chunks` - Chunked text content
- `text_embeddings` - 384-dim vectors
- `image_embeddings` - 512-dim vectors
- `tags` - User-defined labels
- `document_tags` - Many-to-many relationships
- `watch_folders` - Monitored directories

See [ARCHITECTURE.md](./ARCHITECTURE.md) for detailed schema.

## 🔌 IPC Commands

Call these from the frontend using `@tauri-apps/api`:

```typescript
import { invoke } from '@tauri-apps/api/core';

// Initialize database
await invoke('initialize_database');

// Initialize ML models
await invoke('initialize_models');

// Search documents
const results = await invoke('search_documents', {
  options: {
    query: 'vacation photos',
    limit: 20
  }
});

// Index a file
await invoke('index_file', { path: '/path/to/file' });

// Get/save config
const config = await invoke('get_config');
await invoke('save_config', { config });
```

## 🧪 Development

### Watch Mode

```bash
# Terminal 1: Tauri dev mode (frontend + backend)
npm run tauri:dev

# Terminal 2: Watch Rust files
cd src-tauri
cargo watch -x check
```

### Building

```bash
# Development build
cargo build

# Release build
cargo build --release

# Create installer
npm run tauri:build
```

### Testing

```bash
# Rust tests
cd src-tauri
cargo test

# Frontend tests
npm test
```

## 📋 Current Implementation Status

### ✅ Completed
- Project structure and configuration
- Database schema and initialization
- Service layer foundations
- IPC command stubs
- Complete documentation

### 🚧 In Progress
- Search implementation
- File indexing pipeline
- Frontend UI components

### 📝 Planned
- Vector similarity search
- Full-text search
- Hybrid search (vector + text)
- File watcher integration
- Settings UI
- Results display
- Cloud sync (optional)

See [PROJECT_STATUS.md](./PROJECT_STATUS.md) for detailed progress.

## 🔒 Privacy & Security

**Local-First by Design:**
- All data stored locally (SQLite)
- No telemetry or analytics
- No cloud dependencies by default
- Optional cloud sync (user-controlled)

**File System Safety:**
- Read-only access to user files
- Never modifies original files
- Sandboxed to configured directories

**Data Protection:**
- Parameterized SQL queries (no injection)
- No hardcoded secrets
- User controls all data

## 🙏 Acknowledgments

Built with:
- [Tauri](https://tauri.app/) - Desktop framework
- [SQLx](https://github.com/launchbadge/sqlx) - Async SQL
- [ONNX Runtime](https://onnxruntime.ai/) - ML inference
- [Sentence Transformers](https://www.sbert.net/) - Embeddings
- [OpenAI CLIP](https://github.com/openai/CLIP) - Image embeddings

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

**Vault** - Your memories, your data, your control. 🔒
