/**
 * LLM/Q&A API Types
 *
 * Type definitions for question answering and LLM functionality.
 * These types match the Rust backend structures from commands/qa.rs and qa/types.rs
 */

export interface QAResponse {
  answer: string;
  sources: SourceReference[];
}

export interface SourceReference {
  file_path: string;
  score: number;
  snippet: string;
}

export type StreamChunk =
  | { type: 'token'; content: string }
  | { type: 'sources'; sources: SourceReference[] }
  | { type: 'done' }
  | { type: 'error'; message: string };

export interface LLMHealthStatus {
  available: boolean;
  model: string;
  backend: string;
}
