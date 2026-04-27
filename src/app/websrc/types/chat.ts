export type MessageRole = 'user' | 'assistant' | 'system';

export interface Message {
  id: string;
  role: MessageRole;
  content: string;
  timestamp: number;
  streaming?: boolean;
  error?: string;
}

export interface ChatState {
  messages: Message[];
  isThinking: boolean;
  error: string | null;
}

export interface ChatResponse {
  message: string;
  is_streaming: boolean;
}
