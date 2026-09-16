export interface DownloadedModel {
  id: string;
  model_name: string;
  model_id: string;
  file_path: string;
  file_size_bytes: number;
  downloaded_at: string;
  last_used_at: string | null;
  use_count: number;
  is_active_for_chat: boolean;
  is_active_for_embedding: boolean;
  is_active_for_utility: boolean;
  backend: 'local' | 'ollama';
  model_type: string; // "text_embeddings" | "vision" | "reranker" | "language_model"
  metadata: Record<string, unknown> | null;
}
