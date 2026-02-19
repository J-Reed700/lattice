/**
 * Embeddings API Types
 *
 * Type definitions for embedding generation.
 * These types match the Rust backend structures from commands/embeddings.rs
 */

export interface EmbeddingModelInfo {
  model_name: string;
  dimension: number;      // Singular, matches Rust EmbeddingModelInfoDto
  max_tokens: number;     // Matches Rust, NOT is_loaded
}
