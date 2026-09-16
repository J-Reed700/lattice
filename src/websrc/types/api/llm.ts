/**
 * LLM/Q&A API Types
 *
 * Type definitions for question answering and LLM functionality.
 * These types match the Rust backend structures from commands/qa.rs and qa/types.rs
 */

export type QAResponse = import('../../lib/bindings').QAResponseDto;

export type SourceReference = import('../../lib/bindings').SourceDto;

export type StreamChunk =
  | { type: 'token'; content: string }
  | { type: 'sources'; sources: SourceReference[] }
  | { type: 'done' }
  | { type: 'error'; message: string };

export type LLMHealthStatus = import('../../lib/bindings').LLMHealthStatusDto;
