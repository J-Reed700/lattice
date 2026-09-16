/**
 * Progress Types
 *
 * Type definitions for the progress tracking system.
 * Supports multiple concurrent operations with real-time updates.
 */

export type OperationType = 'upload' | 'indexing' | 'search' | 'export' | 'ocr';

export type OperationStatus =
  | 'pending'
  | 'running'
  | 'completed'
  | 'failed'
  | 'cancelled';

export interface ProgressOperation {
  id: string;
  type: OperationType;
  status: OperationStatus;
  progress: number; // 0-100
  current: number;
  total: number;
  message: string;
  startTime: Date;
  endTime?: Date;
  eta?: number; // seconds remaining
  cancellable: boolean;
  errors?: string[];
  metadata?: Record<string, unknown>;
}

export interface ProgressUpdate {
  id: string;
  progress?: number;
  current?: number;
  total?: number;
  message?: string;
  status?: OperationStatus;
  eta?: number;
  errors?: string[];
}

export interface CreateOperationParams {
  type: OperationType;
  message: string;
  total?: number;
  cancellable?: boolean;
  metadata?: Record<string, unknown>;
}

export interface ProgressNotification {
  id: string;
  operationId: string;
  message: string;
  type: 'success' | 'error' | 'info';
  duration?: number;
  action?: {
    label: string;
    onClick: () => void;
  };
}

export interface ProgressHistory {
  operation: ProgressOperation;
  timestamp: Date;
}
