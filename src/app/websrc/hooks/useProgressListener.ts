/**
 * useProgressListener Hook
 *
 * Listens to Tauri backend progress events and updates the progress store.
 * Supports multiple event types: upload, indexing, search, export, OCR.
 */

import { useEffect, useRef } from 'react';

import { type UnlistenFn } from '@tauri-apps/api/event';

import { useProgressStore } from '../stores/progressStore';
import { EventSchemas, listenValidated } from '../types/events';
import { type OperationType } from '../types/progress';

export interface ProgressListenerOptions {
  /** Operation types to listen for */
  types?: OperationType[];
  /** Auto-create operations if not exists */
  autoCreate?: boolean;
}

export function useProgressListener(options: ProgressListenerOptions = {}) {
  const { types = ['upload', 'indexing', 'search', 'export', 'ocr'], autoCreate = true } = options;

  const {
    createOperation,
    updateOperation,
    completeOperation,
    failOperation,
  } = useProgressStore();

  const operationMapRef = useRef<Map<string, string>>(new Map());

  useEffect(() => {
    let isMounted = true;
    const unlisteners: UnlistenFn[] = [];
    const operationMap = operationMapRef.current;

    const setupListeners = async () => {
      // Listen to progress events for each type
      for (const type of types) {
        if (!isMounted) break; // Stop if component unmounted

        // Progress updates with runtime validation
        const progressUnlisten = await listenValidated(
          `${type}-progress`,
          EventSchemas.Progress.ProgressEvent,
          (event) => {
            const payload = event.payload;
            let operationId = payload.operationId;

            // Auto-create operation if needed
            if (!operationId && autoCreate) {
              const key = `${type}-${payload.filename || 'operation'}`;
              operationId = operationMapRef.current.get(key);

              if (!operationId) {
                operationId = createOperation({
                  type,
                  message: payload.message || payload.filename || `${type} in progress`,
                  total: payload.total,
                  cancellable: type === 'upload' || type === 'indexing',
                });
                operationMapRef.current.set(key, operationId);
              }
            }

            if (operationId) {
              updateOperation({
                id: operationId,
                current: payload.current,
                total: payload.total,
                progress: payload.percentage,
                message: payload.message || payload.filename || '',
                eta: payload.eta_ms ? payload.eta_ms / 1000 : undefined,
                status: 'running',
              });
            }
          },
          (error) => {
            console.error(`[useProgressListener] Progress event validation error for ${type}:`, error.format());
          }
        );
        unlisteners.push(progressUnlisten);

        if (!isMounted) break; // Stop if component unmounted

        // Completion events with runtime validation
        const completeUnlisten = await listenValidated(
          `${type}-complete`,
          EventSchemas.Progress.CompleteEvent,
          (event) => {
            const payload = event.payload;
            let operationId = payload.operationId;

            if (!operationId && autoCreate) {
              const key = `${type}-${payload.path || 'operation'}`;
              operationId = operationMapRef.current.get(key);

              if (operationId) {
                operationMapRef.current.delete(key);
              }
            }

            if (operationId) {
              completeOperation(
                operationId,
                payload.message || `${type} completed successfully`
              );
            }
          },
          (error) => {
            console.error(`[useProgressListener] Complete event validation error for ${type}:`, error.format());
          }
        );
        unlisteners.push(completeUnlisten);

        if (!isMounted) break; // Stop if component unmounted

        // Error events with runtime validation
        const errorUnlisten = await listenValidated(
          `${type}-error`,
          EventSchemas.Progress.ErrorEvent,
          (event) => {
            const payload = event.payload;
            let operationId = payload.operationId;

            if (!operationId && autoCreate) {
              const key = `${type}-${payload.path || 'operation'}`;
              operationId = operationMapRef.current.get(key);

              if (operationId) {
                operationMapRef.current.delete(key);
              }
            }

            if (operationId) {
              failOperation(operationId, payload.message);
            }
          },
          (error) => {
            console.error(`[useProgressListener] Error event validation error for ${type}:`, error.format());
          }
        );
        unlisteners.push(errorUnlisten);
      }
    };

    setupListeners();

    // Cleanup
    return () => {
      isMounted = false;
      unlisteners.forEach((unlisten) => unlisten());
      operationMap.clear();
    };
  }, [types, autoCreate, createOperation, updateOperation, completeOperation, failOperation]);
}

/**
 * Hook to manually track progress for an operation
 */
export function useProgress(type: OperationType, cancellable = false) {
  const { createOperation, updateOperation, completeOperation, failOperation, cancelOperation } =
    useProgressStore();

  const start = (message: string, total = 0) => createOperation({
      type,
      message,
      total,
      cancellable,
    });

  const update = (
    id: string,
    current: number,
    message?: string,
    total?: number,
    eta?: number
  ) => {
    updateOperation({
      id,
      current,
      message,
      total,
      eta,
      status: 'running',
    });
  };

  const complete = (id: string, message?: string) => {
    completeOperation(id, message);
  };

  const fail = (id: string, error: string) => {
    failOperation(id, error);
  };

  const cancel = (id: string) => {
    cancelOperation(id);
  };

  return {
    start,
    update,
    complete,
    fail,
    cancel,
  };
}
