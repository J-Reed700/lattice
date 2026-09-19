# Advanced Features

This guide covers Vault's advanced capabilities for power users who want to get the most out of the application.

## Table of Contents

- [Search Modes Deep Dive](#search-modes-deep-dive)
- [Agentic RAG and AI Features](#agentic-rag-and-ai-features)
- [OCR and Document Processing](#ocr-and-document-processing)
- [Performance Tuning](#performance-tuning)
- [Privacy and Security Settings](#privacy-and-security-settings)
- [Command-Line Interface](#command-line-interface)
- [API and Integrations](#api-and-integrations)
- [Custom Embeddings](#custom-embeddings)
- [Advanced Workflows](#advanced-workflows)

## Search Modes Deep Dive

Understand how each search mode works under the hood and when to use them.

### Semantic Search Architecture

**How it Works:**

1. **Query Processing**
   - Your search query is tokenized into words
   - Stop words (the, a, an) are preserved for context
   - Tokens are converted to numerical IDs

2. **Embedding Generation**
   - The tokenized query is fed into a transformer model
   - The model outputs a vector (embedding) — 1024 numbers for the default
     Qwen3-Embedding-0.6B, 384 for all-MiniLM-L6-v2 on machines without a GPU
   - This vector represents the semantic meaning of your query

3. **Similarity Calculation**
   - Vault compares your query embedding to all file embeddings
   - Uses cosine similarity or dot product
   - Files with similar embeddings score higher

4. **Ranking and Results**
   - Results are sorted by similarity score (0-100)
   - Only results above similarity threshold are shown
   - Metadata is enriched from database

**Screenshot placeholder:** *Diagram showing semantic search pipeline from query to results*

**Advanced Configuration:**

```json
// Settings > Advanced > Search Configuration
{
  "semantic_search": {
    "model": "all-MiniLM-L6-v2",
    "similarity_metric": "cosine",  // or "dot_product"
    "threshold": 0.6,               // 0.0 to 1.0
    "max_results": 100,
    "use_gpu": false,               // Enable if GPU available
    "normalize_embeddings": true
  }
}
```

**Performance Characteristics:**
- **Speed:** ~50-200ms for 100,000 documents
- **Accuracy:** Best for conceptual matches
- **Resource usage:** CPU-bound, ~500 MB RAM
- **Scalability:** Linear with document count

### Keyword Search Architecture

**How it Works:**

1. **Query Parsing**
   - Boolean operators (AND, OR, NOT) are parsed
   - Phrases in quotes are treated as exact matches
   - Wildcards (*) are expanded

2. **Inverted Index Lookup**
   - Vault maintains an inverted index (word → document IDs)
   - Looks up each query term in the index
   - Retrieves matching document IDs

3. **BM25 Scoring**
   - Uses BM25 algorithm for relevance scoring
   - Accounts for term frequency and document length
   - Rare words score higher than common words

4. **Boolean Filtering**
   - Applies AND/OR/NOT logic to filter results
   - Phrase matching checks word proximity
   - Returns final result set

**Screenshot placeholder:** *Diagram showing keyword search with inverted index*

**Advanced Configuration:**

```json
// Settings > Advanced > Keyword Search
{
  "keyword_search": {
    "algorithm": "bm25",
    "k1": 1.5,                      // Term frequency saturation
    "b": 0.75,                      // Length normalization
    "min_word_length": 2,
    "stemming": true,               // Enable word stemming
    "case_sensitive": false
  }
}
```

**BM25 Parameters:**
- **k1:** Controls term frequency saturation (1.2-2.0, default 1.5)
  - Higher = more weight to term frequency
  - Lower = diminishing returns for repeated terms
- **b:** Controls document length normalization (0.0-1.0, default 0.75)
  - 1.0 = Full normalization (penalize long documents)
  - 0.0 = No normalization

### Hybrid Search Architecture

**How it Works:**

1. **Parallel Execution**
   - Semantic and keyword searches run simultaneously
   - Each produces ranked results with scores

2. **Score Normalization**
   - Semantic scores normalized to 0-1 range
   - BM25 scores normalized to 0-1 range
   - Ensures fair comparison

3. **Fusion Algorithm**
   - **Reciprocal Rank Fusion (RRF):**
     ```
     score = 1/(k + semantic_rank) + 1/(k + keyword_rank)
     ```
   - **Weighted Average:**
     ```
     score = α × semantic_score + (1-α) × keyword_score
     ```
   - k is a constant (default: 60)
   - α is the semantic weight (default: 0.5)

4. **Deduplication**
   - Merges results appearing in both searches
   - Keeps highest combined score
   - Preserves diversity

**Screenshot placeholder:** *Hybrid search fusion algorithm visualization*

**Advanced Configuration:**

```json
// Settings > Advanced > Hybrid Search
{
  "hybrid_search": {
    "fusion_method": "rrf",         // "rrf" or "weighted"
    "semantic_weight": 0.5,         // 0.0 to 1.0
    "keyword_weight": 0.5,
    "rrf_k": 60,
    "min_overlap_boost": 1.2,      // Boost docs in both results
    "diversity_penalty": 0.1        // Penalize similar docs
  }
}
```

**When to Use Each Mode:**

| Scenario | Best Mode | Why |
|----------|-----------|-----|
| Finding similar concepts | Semantic | Understands meaning |
| Exact phrase or term | Keyword | Precise matching |
| Complex queries | Hybrid | Best of both worlds |
| Technical documentation | Keyword | Specific terminology |
| Personal notes | Semantic | Natural language |
| Code search | Keyword | Exact identifiers |
| Research papers | Hybrid | Technical + conceptual |

## Agentic RAG and AI Features

Vault includes AI-powered question-answering capabilities using Retrieval-Augmented Generation (RAG).

### What is Agentic RAG?

**Traditional RAG:**
1. Retrieve relevant documents
2. Feed to language model
3. Generate answer

**Agentic RAG (Vault):**
1. Analyze question
2. Plan retrieval strategy
3. Iteratively search and refine
4. Synthesize answer with citations
5. Verify and fact-check

**Screenshot placeholder:** *Diagram comparing traditional RAG vs. agentic RAG*

### Setting Up Q&A

**Prerequisites:**
1. Indexed documents (your knowledge base)
2. LLM backend (Ollama or OpenAI)
3. Sufficient RAM (8 GB minimum)

**Configuration:**

1. **Install Ollama (Local LLM)**
   ```bash
   # Download from https://ollama.ai
   # Install and run:
   ollama serve

   # Pull a model:
   ollama pull llama2
   ```

2. **Configure in Vault**
   - Settings > LLM > Provider: Ollama
   - URL: http://localhost:11434
   - Model: llama2 (or your preferred model)
   - Test connection

3. **Or Use OpenAI**
   - Settings > LLM > Provider: OpenAI
   - Enter API key
   - Model: gpt-3.5-turbo or gpt-4
   - Set usage limits

**Screenshot placeholder:** *LLM settings showing Ollama and OpenAI configuration*

### Using Q&A

**Ask a Question:**

1. Click "Q&A" button or press `Ctrl+Q`
2. Type your question in natural language
3. Vault:
   - Searches your indexed files
   - Retrieves relevant passages
   - Generates an answer using LLM
   - Provides citations

**Example Questions:**
```
"What did the marketing report say about Q3 revenue?"
"Summarize all my meeting notes from last week"
"What are the key findings in my research papers about AI?"
"List all project deadlines mentioned in my documents"
```

**Screenshot placeholder:** *Q&A interface showing question, answer, and source citations*

### Answer Quality and Citations

**Answer Components:**

1. **Direct Answer**
   - Synthesized from your documents
   - Cites specific sources
   - Includes confidence level

2. **Source Citations**
   - Document name and path
   - Relevant excerpt
   - Relevance score
   - Link to open document

3. **Related Documents**
   - Other potentially relevant files
   - Allows further exploration

**Improving Answer Quality:**

- **Better questions:**
  ```
  Bad:  "sales"
  Good: "What were the sales figures for Q3 2024?"
  ```

- **Context in questions:**
  ```
  Bad:  "What did he say?"
  Good: "What did John say in the project meeting notes?"
  ```

- **Specific timeframes:**
  ```
  "According to documents from last month..."
  "In my 2024 notes..."
  ```

**Screenshot placeholder:** *High-quality Q&A result with detailed citations*

### Advanced Q&A Features

**Multi-Step Reasoning**

For complex questions, enable multi-step reasoning:

Settings > LLM > Enable Multi-Step Reasoning

Example:
```
Question: "Based on my meeting notes and project docs,
          what are the biggest risks to completing the
          project on time?"

Vault:
1. Searches meeting notes for timeline discussions
2. Searches project docs for deadlines
3. Searches for mentions of blockers/issues
4. Synthesizes risk analysis
5. Provides recommendations
```

**Conversation Mode**

Enable conversation memory to ask follow-up questions:

```
You: "What are the main points in the quarterly report?"
Vault: [Provides summary]

You: "What about the financial projections?"
Vault: [Uses context from previous question]
```

**Screenshot placeholder:** *Conversation mode showing multi-turn dialogue*

**Summarization**

Summarize long documents or multiple files:

1. Select one or more files
2. Right-click > "Summarize"
3. Choose summary length: Brief, Medium, Detailed
4. Vault generates summary with key points

**Batch Summarization:**
- Settings > Summary > Batch Summarize
- Select folder or search results
- Generate summaries for all files
- Summaries stored and searchable

**Screenshot placeholder:** *Batch summarization interface with progress*

### LLM Configuration

**Model Selection:**

| Model | Size | Speed | Quality | Best For |
|-------|------|-------|---------|----------|
| llama2 | 7B | Fast | Good | General Q&A |
| llama2 | 13B | Medium | Better | Complex questions |
| mistral | 7B | Fast | Good | Technical docs |
| codellama | 7B | Fast | Good | Code questions |
| gpt-3.5-turbo | - | Fast | Great | General Q&A |
| gpt-4 | - | Slow | Excellent | Complex analysis |

**Performance Tuning:**

```json
// Settings > Advanced > LLM Configuration
{
  "model": "llama2",
  "temperature": 0.7,              // Creativity (0-1)
  "max_tokens": 2048,              // Answer length
  "context_window": 4096,          // Max context size
  "top_p": 0.9,                    // Nucleus sampling
  "top_k": 40,                     // Token sampling
  "timeout_seconds": 60,
  "stream_responses": true         // Progressive display
}
```

**Parameters Explained:**
- **Temperature:** Higher = more creative, lower = more focused
- **Max tokens:** Maximum answer length (tokens ≈ words)
- **Context window:** How much source text to include
- **Top-p/Top-k:** Control randomness in generation

## OCR and Document Processing

Extract text from images and scanned documents.

### Setting Up OCR

**Prerequisites:**
1. Tesseract OCR engine
2. Language data files
3. Sufficient disk space for processed text

**Installation:**

**Windows:**
```powershell
# Download from https://github.com/UB-Mannheim/tesseract/wiki
# Run installer
# Add to PATH: C:\Program Files\Tesseract-OCR

# Verify installation
tesseract --version
```

**macOS:**
```bash
brew install tesseract
brew install tesseract-lang  # Additional languages
```

**Linux:**
```bash
sudo apt-get install tesseract-ocr
sudo apt-get install tesseract-ocr-eng  # English
sudo apt-get install tesseract-ocr-fra  # French, etc.
```

**Configure in Vault:**
1. Settings > OCR > Enable OCR
2. Tesseract path: (auto-detected or manual)
3. Languages: Select languages to recognize
4. OCR quality: Fast, Balanced, or Best

**Screenshot placeholder:** *OCR settings showing Tesseract configuration*

### OCR Capabilities

**Supported Image Types:**
- Scanned documents (PDF, TIFF)
- Photos of documents (JPG, PNG)
- Screenshots with text
- Handwritten text (limited accuracy)

**Text Extraction Quality:**
- **Printed text:** 95-99% accuracy
- **Clear scans:** 90-95% accuracy
- **Photos:** 80-90% accuracy
- **Handwriting:** 60-80% accuracy

**Language Support:**
- 100+ languages supported
- Multiple languages per document
- Auto-detection available

### Advanced OCR Features

**Preprocessing:**

Improve OCR accuracy with image preprocessing:

Settings > OCR > Preprocessing
- **Deskew:** Straighten rotated images
- **Denoise:** Remove background noise
- **Contrast:** Enhance text contrast
- **Binarization:** Convert to black and white
- **Sharpening:** Sharpen blurry text

**Screenshot placeholder:** *Before/after preprocessing comparison*

**Layout Analysis:**

Preserve document structure:
- Detect columns and sections
- Maintain reading order
- Identify headers and footers
- Preserve tables (basic)

**Batch OCR:**

Process multiple images at once:
1. Select images or folder
2. Right-click > "OCR Batch"
3. Configure settings
4. Monitor progress
5. Review results

**Screenshot placeholder:** *Batch OCR interface showing queue*

## Performance Tuning

Optimize Vault for speed and efficiency.

### Indexing Performance

**Benchmark Your System:**

Settings > Advanced > Run Benchmark

Measures:
- Embedding generation speed
- Database write speed
- File reading speed
- Overall throughput

**Screenshot placeholder:** *Benchmark results showing system performance*

**Optimization Strategies:**

**1. Adjust Batch Size**
```
Small batch (8-16):   Lower memory, slower
Medium batch (32):    Balanced (default)
Large batch (64-128): Higher memory, faster
```

Test different sizes to find optimal for your system.

**2. Thread Allocation**
```
Auto-detect:     Uses all available cores
Manual (4-8):    Leave cores for other apps
High (12-16):    Maximum speed
```

**3. Prioritization**
```
Background:  Minimal impact on other apps
Normal:      Balanced priority
High:        Fastest indexing, may affect responsiveness
```

**4. Content Analysis Depth**
```
Quick:       Basic text extraction
Standard:    Full text + basic metadata
Deep:        Full analysis + advanced features
```

**Screenshot placeholder:** *Performance settings with optimization recommendations*

### Search Performance

**Index Optimization:**

Run periodically for best search speed:
```
Settings > Advanced > Optimize Database
- Rebuilds search indexes
- Compacts database
- Updates statistics
```

**Caching Strategies:**

**Query Cache:**
- Remembers recent searches
- Instant results for repeated queries
- Configurable size and TTL

**Embedding Cache:**
- Caches query embeddings
- Reuses for similar queries
- Saves computation time

**Metadata Cache:**
- Caches file metadata
- Reduces database queries
- Auto-invalidates on changes

**Screenshot placeholder:** *Cache statistics showing hit rates*

### Database Performance

**Connection Pooling:**

```json
// Settings > Advanced > Database
{
  "connection_pool_size": 10,      // Concurrent connections
  "connection_timeout": 30,        // Seconds
  "max_lifetime": 3600,            // Seconds
  "idle_timeout": 600              // Seconds
}
```

**Write-Ahead Logging (WAL):**

Enabled by default for better concurrent performance:
- Faster writes
- Better concurrency
- Minimal read impact

**Pragma Settings:**

```sql
-- Auto-configured by Vault
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA cache_size = 10000;
PRAGMA temp_store = MEMORY;
PRAGMA mmap_size = 30000000000;
```

**Vacuum and Analysis:**

```
Settings > Advanced > Maintenance
- Vacuum: Reclaim space (run monthly)
- Analyze: Update query planner statistics (run weekly)
```

### Memory Management

**Memory Usage Breakdown:**
- Embeddings cache: ~200 MB
- Search results: ~50 MB per 1000 results
- Database connections: ~10 MB each
- UI and application: ~100-200 MB

**Reducing Memory Usage:**

1. Lower cache sizes
2. Reduce max results
3. Close when not needed
4. Disable thumbnails
5. Use smaller embedding models

**Monitoring:**

Settings > Advanced > Resource Monitor
- Real-time memory usage
- CPU utilization
- Disk I/O
- Network (if applicable)

**Screenshot placeholder:** *Resource monitor showing real-time graphs*

## Privacy and Security Settings

Protect your data and maintain privacy.

### Data Encryption

**Database Encryption:**

Encrypt your vault database:

1. Settings > Security > Enable Encryption
2. Choose encryption method:
   - **AES-256:** Industry standard, fast
   - **ChaCha20:** Faster on mobile, equally secure
3. Set master password
4. Confirm password
5. Vault encrypts database (may take time)

**Important:**
- Password cannot be recovered if lost
- Create backup before encrypting
- Performance impact: ~10-20% slower

**Screenshot placeholder:** *Encryption setup wizard*

**File-Level Encryption:**

For extra security, encrypt individual files:

1. Select files
2. Right-click > "Encrypt"
3. Enter password
4. Files encrypted in-place
5. Searchable only when unlocked

### Access Control

**Password Protection:**

Require password to open Vault:

Settings > Security > Password Protection
- Enable password on launch
- Set complexity requirements
- Configure unlock timeout
- Enable biometric (if available)

**Auto-Lock:**

Lock Vault after inactivity:

Settings > Security > Auto-Lock
- Lock after: 5, 10, 15, 30 min, or Never
- Lock on sleep
- Lock on screen saver
- Require password to unlock

**Screenshot placeholder:** *Auto-lock settings*

### Privacy Features

**Disable Telemetry:**

Already disabled by default. Vault collects zero data.

Verify: Settings > Privacy > Telemetry: OFF

**Network Requests:**

Control when Vault accesses network:

Settings > Privacy > Network
- Update checks: On/Off
- Model downloads: On/Off
- Never send document content
- Never send search queries

**Audit Log:**

Track all access to your vault:

Settings > Security > Enable Audit Log
- Records all operations
- Timestamped entries
- Exportable for review
- Auto-rotation

**Screenshot placeholder:** *Audit log showing recent activities*

### Secure Deletion

**Remove from Index:**

Remove file from search without deleting:
- Right-click > Remove from Index
- File stays on disk
- No longer searchable

**Secure Delete:**

Permanently delete files:
- Right-click > Secure Delete
- Overwrites file data (3 passes)
- Unrecoverable
- Requires confirmation

**Clear Cache Securely:**

Settings > Privacy > Secure Clear Cache
- Overwrites cache data
- Clears search history
- Removes temporary files

## Command-Line Interface

Advanced users can control Vault via command line.

### CLI Installation

The CLI is included with Vault. Add to PATH:

**Windows:**
```powershell
# Add to PATH
$env:PATH += ";C:\Program Files\Vault"

# Verify
vault --version
```

**macOS/Linux:**
```bash
# Add to PATH (add to .bashrc or .zshrc)
export PATH="$PATH:/Applications/Vault.app/Contents/MacOS"

# Verify
vault --version
```

### CLI Commands

**Indexing:**
```bash
# Index a folder
vault index /path/to/folder

# Index recursively
vault index --recursive /path/to/folder

# Reindex all
vault reindex

# Index single file
vault index-file /path/to/file.pdf
```

**Searching:**
```bash
# Semantic search
vault search "query"

# Keyword search
vault search --mode keyword "exact phrase"

# Hybrid search
vault search --mode hybrid "query"

# Limit results
vault search --limit 10 "query"

# Export results
vault search "query" --format json > results.json
```

**Management:**
```bash
# Show stats
vault stats

# List indexed folders
vault list-folders

# Add watch folder
vault add-folder /path/to/watch

# Remove folder
vault remove-folder /path/to/watch

# Optimize database
vault optimize

# Create backup
vault backup /path/to/backup.zip

# Restore backup
vault restore /path/to/backup.zip
```

**Screenshot placeholder:** *Terminal showing CLI commands in action*

### Automation Scripts

**Daily Backup Script:**

```bash
#!/bin/bash
# daily-backup.sh

BACKUP_DIR="$HOME/vault-backups"
DATE=$(date +%Y-%m-%d)
BACKUP_FILE="$BACKUP_DIR/vault-$DATE.zip"

# Create backup
vault backup "$BACKUP_FILE"

# Keep only last 30 days
find "$BACKUP_DIR" -name "vault-*.zip" -mtime +30 -delete

echo "Backup completed: $BACKUP_FILE"
```

**Auto-Index New Files:**

```bash
#!/bin/bash
# watch-and-index.sh

WATCH_DIR="$HOME/Documents"

# Watch for new files
fswatch -0 "$WATCH_DIR" | while read -d "" file
do
  vault index-file "$file"
  echo "Indexed: $file"
done
```

**Weekly Maintenance:**

```bash
#!/bin/bash
# weekly-maintenance.sh

vault optimize
vault vacuum
vault verify-integrity

echo "Maintenance completed"
```

### Scripting with Python

```python
import subprocess
import json

def vault_search(query, limit=10):
    """Search Vault and return JSON results"""
    result = subprocess.run(
        ['vault', 'search', '--format', 'json', '--limit', str(limit), query],
        capture_output=True,
        text=True
    )
    return json.loads(result.stdout)

# Example usage
results = vault_search("meeting notes", limit=5)
for result in results:
    print(f"{result['filename']}: {result['score']}")
```

## API and Integrations

Integrate Vault with other applications.

### REST API

Enable the REST API:

Settings > Advanced > Enable REST API
- Port: 9988 (configurable)
- API Key: (auto-generated)
- CORS: Configure allowed origins

**API Endpoints:**

```
GET  /api/v1/search?q=query&limit=10
POST /api/v1/index
GET  /api/v1/documents/:id
POST /api/v1/documents/:id/tags
GET  /api/v1/stats
```

**Example Request:**

```bash
curl -H "X-API-Key: YOUR_KEY" \
  "http://localhost:9988/api/v1/search?q=meeting+notes&limit=5"
```

**Response:**
```json
{
  "results": [
    {
      "id": "doc-123",
      "filename": "meeting-notes-2024.md",
      "score": 0.92,
      "snippet": "Discussed project timeline...",
      "metadata": { ... }
    }
  ],
  "total": 5,
  "query_time_ms": 45
}
```

**Screenshot placeholder:** *API documentation viewer*

### Browser Extension

Install the Vault browser extension (Chrome, Firefox, Edge):

**Features:**
- Save web pages to Vault
- Search Vault from browser
- Quick capture to daily notes
- Search with selected text

**Screenshot placeholder:** *Browser extension popup*

### App Integrations

**Obsidian Plugin:**

Install "Vault Connector" from Obsidian community plugins:
- Search Vault from Obsidian
- Link to indexed files
- Sync tags and metadata

**VS Code Extension:**

Install "Vault Search" from VS Code marketplace:
- Search code and docs
- Open files in VS Code
- Index workspace

**Alfred Workflow (macOS):**

Download from Vault website:
- Instant search with `v query`
- Quick actions
- Keyboard shortcuts

**Screenshot placeholder:** *Integrations overview diagram*

## Custom Embeddings

Advanced users can use custom embedding models.

### Why Custom Models?

- **Domain-specific:** Models trained for your field
- **Better accuracy:** Optimized for your content
- **Privacy:** Self-hosted models
- **Performance:** Smaller, faster models

### Using Custom Models

**ONNX Models:**

1. Obtain or train ONNX model
2. Convert to ONNX format if needed
3. Place in models directory
4. Configure in settings

**Configuration:**

```json
// Settings > Advanced > Custom Embedding Model
{
  "model_path": "/path/to/model.onnx",
  "tokenizer_path": "/path/to/tokenizer.json",
  "dimension": 384,
  "max_seq_length": 512,
  "pooling": "mean"  // or "cls"
}
```

**Supported Models:**
- Sentence Transformers (all models)
- BERT variants
- RoBERTa, DeBERTa
- Custom fine-tuned models

### Model Training

Train a custom model for your documents:

1. Export your indexed text
2. Create training dataset
3. Fine-tune base model
4. Export to ONNX
5. Import to Vault

**Resources:**
- [Training guide](https://github.com/yourusername/vault/wiki/Custom-Models)
- [Example notebooks](https://github.com/yourusername/vault-models)

**Screenshot placeholder:** *Custom model configuration*

## Advanced Workflows

Complex workflows for power users.

### Research Workflow

1. **Collect Sources**
   - Index research papers folder
   - Tag by topic, methodology, year

2. **Literature Review**
   - Search for topics: "machine learning interpretability"
   - Filter by year, citation count
   - Export citations

3. **Analysis**
   - Ask Q&A: "What are common evaluation metrics?"
   - Summarize key findings
   - Generate literature map

4. **Writing**
   - Search for supporting quotes
   - Link to sources
   - Export references

**Screenshot placeholder:** *Research workflow diagram*

### Project Management

1. **Index Project Files**
   - Meeting notes
   - Project docs
   - Email exports

2. **Daily Standup**
   - Search: "tasks mentioned yesterday"
   - Review action items
   - Update status

3. **Weekly Review**
   - Search: "files modified this week"
   - Identify bottlenecks
   - Plan next week

4. **Reporting**
   - Q&A: "What progress on X?"
   - Generate summary
   - Export report

### Knowledge Management

1. **Capture**
   - Daily notes
   - Quick captures
   - Auto-indexed folders

2. **Organize**
   - Tag important items
   - Create folder structure
   - Link related docs

3. **Review**
   - Weekly: Review new captures
   - Monthly: Organize and archive
   - Quarterly: Prune and optimize

4. **Retrieve**
   - Semantic search
   - Saved searches
   - Quick links

**Screenshot placeholder:** *Knowledge management workflow*

---

## Next Steps

You've learned about Vault's advanced features:
- Master all three search modes
- Set up AI-powered Q&A
- Configure OCR for scanned docs
- Tune performance for your system
- Secure your data
- Automate with CLI and API

**Explore More:**
- [Getting Started](getting-started.md) - Installation guide
- [User Manual](user-manual.md) - Complete feature reference
- [GitHub](https://github.com/yourusername/vault) - Source code and issues

Happy power-using with Vault!
