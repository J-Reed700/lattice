import { describe, it, expect, beforeEach } from 'vitest';

import {
  type TauriEvents,
  TauriEventNames,
  EventSchemas,
} from '@/types/events';

import { MockTauriEventEmitter } from '../utils/MockTauriEventEmitter';

describe('Download Event Flow Integration Tests', () => {
  let emitter: MockTauriEventEmitter;

  beforeEach(() => {
    emitter = new MockTauriEventEmitter();
  });

  describe('Complete download lifecycle', () => {
    it('should emit events in correct order and track state transitions', async () => {
      const events: string[] = [];
      const downloadId = 'test-download-1';

      await emitter.listen<TauriEvents.Downloads.Started>(
        TauriEventNames.Downloads.Started,
        (event) => {
          events.push('started');
          expect(event.payload.id).toBe(downloadId);
        }
      );

      await emitter.listen<TauriEvents.Downloads.Progress>(
        TauriEventNames.Downloads.Progress,
        (event) => {
          events.push('progress');
          expect(event.payload.id).toBe(downloadId);
        }
      );

      await emitter.listen<TauriEvents.Downloads.Completed>(
        TauriEventNames.Downloads.Completed,
        (event) => {
          events.push('completed');
          expect(event.payload.id).toBe(downloadId);
        }
      );

      await emitter.emit<TauriEvents.Downloads.Started>(
        TauriEventNames.Downloads.Started,
        {
          id: downloadId,
          url: 'https://example.com/model',
          destination: '/path/to/model',
          total_bytes: 1000,
          model_id: 'test-model',
        }
      );

      await emitter.emit<TauriEvents.Downloads.Progress>(
        TauriEventNames.Downloads.Progress,
        {
          id: downloadId,
          bytes_downloaded: 500,
          total_bytes: 1000,
          bytes_per_second: 100,
          eta_seconds: 5,
        }
      );

      await emitter.emit<TauriEvents.Downloads.Progress>(
        TauriEventNames.Downloads.Progress,
        {
          id: downloadId,
          bytes_downloaded: 1000,
          total_bytes: 1000,
          bytes_per_second: 100,
          eta_seconds: 0,
        }
      );

      await emitter.emit<TauriEvents.Downloads.Completed>(
        TauriEventNames.Downloads.Completed,
        {
          id: downloadId,
          total_bytes_completed: 1000,
          elapsed_seconds: 10,
        }
      );

      expect(events).toEqual(['started', 'progress', 'progress', 'completed']);
    });
  });

  describe('Download failure', () => {
    it('should capture error message correctly on failure', async () => {
      const downloadId = 'test-download-2';
      let capturedError: string | undefined;

      await emitter.listen<TauriEvents.Downloads.Failed>(
        TauriEventNames.Downloads.Failed,
        (event) => {
          capturedError = event.payload.error;
          expect(event.payload.id).toBe(downloadId);
        }
      );

      await emitter.emit<TauriEvents.Downloads.Failed>(
        TauriEventNames.Downloads.Failed,
        {
          id: downloadId,
          error: 'Network error: Connection timeout',
        }
      );

      expect(capturedError).toBe('Network error: Connection timeout');
    });
  });

  describe('Session routing', () => {
    it('should route events to correct downloads', async () => {
      const download1 = 'download-1';
      const download2 = 'download-2';
      const download1Events: string[] = [];
      const download2Events: string[] = [];

      await emitter.listen<TauriEvents.Downloads.Progress>(
        TauriEventNames.Downloads.Progress,
        (event) => {
          if (event.payload.id === download1) {
            download1Events.push(`progress-${event.payload.bytes_downloaded}`);
          } else if (event.payload.id === download2) {
            download2Events.push(`progress-${event.payload.bytes_downloaded}`);
          }
        }
      );

      await emitter.emit<TauriEvents.Downloads.Progress>(
        TauriEventNames.Downloads.Progress,
        {
          id: download1,
          bytes_downloaded: 100,
          total_bytes: 1000,
          bytes_per_second: 50,
          eta_seconds: 18,
        }
      );

      await emitter.emit<TauriEvents.Downloads.Progress>(
        TauriEventNames.Downloads.Progress,
        {
          id: download2,
          bytes_downloaded: 200,
          total_bytes: 2000,
          bytes_per_second: 100,
          eta_seconds: 18,
        }
      );

      await emitter.emit<TauriEvents.Downloads.Progress>(
        TauriEventNames.Downloads.Progress,
        {
          id: download1,
          bytes_downloaded: 500,
          total_bytes: 1000,
          bytes_per_second: 50,
          eta_seconds: 10,
        }
      );

      await emitter.emit<TauriEvents.Downloads.Progress>(
        TauriEventNames.Downloads.Progress,
        {
          id: download2,
          bytes_downloaded: 1000,
          total_bytes: 2000,
          bytes_per_second: 100,
          eta_seconds: 10,
        }
      );

      expect(download1Events).toEqual(['progress-100', 'progress-500']);
      expect(download2Events).toEqual(['progress-200', 'progress-1000']);
    });
  });

  describe('Race condition (DB commit before event)', () => {
    it('should eventually receive event even with delayed emission', async () => {
      const downloadId = 'test-download-3';
      let eventReceived = false;

      const unlisten = await emitter.listen<TauriEvents.Downloads.Completed>(
        TauriEventNames.Downloads.Completed,
        (event) => {
          eventReceived = true;
          expect(event.payload.id).toBe(downloadId);
        }
      );

      await new Promise((resolve) => setTimeout(resolve, 50));

      await emitter.emit<TauriEvents.Downloads.Completed>(
        TauriEventNames.Downloads.Completed,
        {
          id: downloadId,
          total_bytes_completed: 1000,
          elapsed_seconds: 10,
        }
      );

      expect(eventReceived).toBe(true);

      unlisten();
    });
  });

  describe('Listener cleanup', () => {
    it('should only call handler before cleanup, not after', async () => {
      const downloadId = 'test-download-4';
      let callCount = 0;

      const unlisten = await emitter.listen<TauriEvents.Downloads.Progress>(
        TauriEventNames.Downloads.Progress,
        () => {
          callCount++;
        }
      );

      await emitter.emit<TauriEvents.Downloads.Progress>(
        TauriEventNames.Downloads.Progress,
        {
          id: downloadId,
          bytes_downloaded: 100,
          total_bytes: 1000,
          bytes_per_second: 50,
          eta_seconds: 18,
        }
      );

      expect(callCount).toBe(1);

      unlisten();

      await emitter.emit<TauriEvents.Downloads.Progress>(
        TauriEventNames.Downloads.Progress,
        {
          id: downloadId,
          bytes_downloaded: 500,
          total_bytes: 1000,
          bytes_per_second: 50,
          eta_seconds: 10,
        }
      );

      expect(callCount).toBe(1);
    });
  });

  describe('Runtime validation (Zod)', () => {
    it('should accept valid event payloads', () => {
      const validPayload = {
        id: 'test-download-5',
        bytes_downloaded: 500,
        total_bytes: 1000,
        bytes_per_second: 100,
        percentage: 50,
        eta_seconds: 5,
      };

      const result = EventSchemas.Downloads.Progress.safeParse(validPayload);

      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.id).toBe('test-download-5');
        expect(result.data.bytes_downloaded).toBe(500);
      }
    });

    it('should reject invalid event payloads', () => {
      // Missing required fields
      const invalidPayload = {
        id: 'test-download-6',
        // Missing: bytes_downloaded, bytes_per_second
      };

      const result = EventSchemas.Downloads.Progress.safeParse(invalidPayload);

      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.error.issues.length).toBeGreaterThan(0);
        expect(
          result.error.issues.some((issue) => issue.path.includes('bytes_downloaded'))
        ).toBe(true);
      }
    });

    it('should reject payload with wrong types', () => {
      const invalidPayload = {
        id: 'test-download-7',
        bytes_downloaded: 'not a number', // Wrong type
        total_bytes: 1000,
        bytes_per_second: 100,
      };

      const result = EventSchemas.Downloads.Progress.safeParse(invalidPayload);

      expect(result.success).toBe(false);
    });
  });
});
