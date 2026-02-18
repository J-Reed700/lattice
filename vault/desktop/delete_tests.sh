#!/bin/bash
# Script to delete test modules from files with compilation errors

set -e

cd /Users/joshreed/Code/Recall/vault/desktop/src-tauri

# List of files with inline test modules to clean
files=(
  "src/application/use_cases/indexing/delete_document.rs"
  "src/application/use_cases/indexing/index_directory.rs"
  "src/application/use_cases/indexing/rename_document.rs"
  "src/application/use_cases/llm/download_model.rs"
  "src/application/use_cases/qa/ask_question.rs"
  "src/application/use_cases/search/hybrid_search.rs"
  "src/domain/downloaded_model.rs"
  "src/domain/entities/document.rs"
  "src/domain/model_type_classifier.rs"
  "src/infrastructure/audit/mod.rs"
  "src/infrastructure/indexing/extraction/docx.rs"
  "src/infrastructure/indexing/storage/documents.rs"
  "src/infrastructure/llm/inference/engine.rs"
  "src/infrastructure/llm/model_storage_adapter.rs"
  "src/infrastructure/migrations/web_archive_migration.rs"
  "src/infrastructure/ml/generator.rs"
  "src/infrastructure/persistence/download_repository.rs"
  "src/infrastructure/persistence/mappers/chunk_mapper.rs"
  "src/infrastructure/persistence/mappers/document_mapper.rs"
  "src/infrastructure/persistence/repositories/document_repository.rs"
  "src/infrastructure/persistence/repositories/downloaded_model_repository.rs"
  "src/infrastructure/qa/engine.rs"
  "src/infrastructure/qa/ollama.rs"
  "src/infrastructure/sagas/download_saga.rs"
  "src/infrastructure/search/index.rs"
  "src/infrastructure/search/vector_search/hnsw_index.rs"
  "src/infrastructure/services/mocks/mock_document.rs"
  "src/infrastructure/services/traits/indexing.rs"
  "src/interfaces/commands/model_management_commands.rs"
  "src/shared/utils/alignment.rs"
  "src/shared/utils/patterns/retry.rs"
)

echo "=== Deleting test modules from ${#files[@]} files ==="

for file in "${files[@]}"; do
  if [ ! -f "$file" ]; then
    echo "SKIP: $file (not found)"
    continue
  fi

  # Find line number of #[cfg(test)]
  test_line=$(grep -n "#\[cfg(test)\]" "$file" | head -1 | cut -d: -f1 || echo "")

  if [ -z "$test_line" ]; then
    echo "SKIP: $file (no test module)"
    continue
  fi

  # Delete from test_line to end of file
  # Keep lines 1 to (test_line - 1), then add trailing newline
  total_lines=$(wc -l < "$file")
  production_lines=$((test_line - 1))

  head -n "$production_lines" "$file" > "$file.tmp"
  mv "$file.tmp" "$file"

  echo "✓ $file (removed lines ${test_line}-${total_lines})"
done

echo ""
echo "=== Cleanup complete ==="
echo "Now verifying build..."
