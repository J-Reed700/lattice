import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type ParsedDraft = any;

export type DraftState =
  | { status: 'idle' }
  | { status: 'drafting', progress: string }
  | { status: 'complete', result: ParsedDraft }
  | { status: 'error', error: string };

type DraftUpdateEvent = { requestId: string; content: string };

export function useDrafting() {
  const [state, setState] = useState<DraftState>({ status: 'idle' });
  const activeRequestIdRef = useRef<string | null>(null);
  const bufferRef = useRef<string>('');
  const lastProgressTimeRef = useRef<number>(Date.now());

  useEffect(() => {
    let unlistenProgress: any;
    let unlistenComplete: any;
    let unlistenError: any;

    async function setup() {
      unlistenProgress = await listen<DraftUpdateEvent>('draft-progress', (event) => {
        if (event.payload.requestId !== activeRequestIdRef.current) return;
        
        bufferRef.current += event.payload.content;
        lastProgressTimeRef.current = Date.now();
      });
      unlistenComplete = await listen<DraftUpdateEvent>('draft-complete', (event) => {
        if (event.payload.requestId !== activeRequestIdRef.current) return;
        
        setState({ status: 'complete', result: JSON.parse(event.payload.content) });
        activeRequestIdRef.current = null; // Clear active ID
      });
      unlistenError = await listen<DraftUpdateEvent>('draft-error', (event) => {
        if (event.payload.requestId !== activeRequestIdRef.current) return;
        
        setState({ status: 'error', error: event.payload.content });
        activeRequestIdRef.current = null;
      });
    }

    setup();

    // Throttle progress updates
    const throttleInterval = setInterval(() => {
      if (bufferRef.current && activeRequestIdRef.current) {
        setState(prev => {
          if (prev.status !== 'drafting') return prev;
          return {
            ...prev,
            progress: prev.progress + bufferRef.current
          };
        });
        bufferRef.current = '';
      }
    }, 200);

    // Watchdog
    const watchdogInterval = setInterval(() => {
      if (activeRequestIdRef.current && (Date.now() - lastProgressTimeRef.current > 10000)) {
        setState({ status: 'error', error: 'Drafting timed out' });
        activeRequestIdRef.current = null;
      }
    }, 1000);

    return () => {
      if (unlistenProgress) unlistenProgress();
      if (unlistenComplete) unlistenComplete();
      if (unlistenError) unlistenError();
      clearInterval(throttleInterval);
      clearInterval(watchdogInterval);
    };
  }, []);

  const startDrafting = async (input: string, brief: string) => {
    const requestId = crypto.randomUUID();
    activeRequestIdRef.current = requestId;
    lastProgressTimeRef.current = Date.now();
    setState({ status: 'drafting', progress: '' });
    try {
      await invoke('plugin:learning|start_drafting', { requestId, input, brief });
    } catch (e) {
      setState({ status: 'error', error: String(e) });
      activeRequestIdRef.current = null;
    }
  };

  return { state, startDrafting };
}
