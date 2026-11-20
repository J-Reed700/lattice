import { describe, expect, it } from 'vitest';

import type { ModelOrigin, TurnModelDto } from '@/lib/bindings';
import {
  RetrievalTraceSchema,
  TurnModelSchema,
  TurnRecordSchema,
  TurnStepSchema,
} from '@/types/conversation';

describe('TurnModelSchema', () => {
  it('reads a record written before turns named their model', () => {
    expect(TurnModelSchema.parse({ id: 'qwen', name: 'Qwen3.5-27B' })).toEqual({ id: 'qwen', name: 'Qwen3.5-27B' });
  });

  it('reads every field the backend sends, including every ModelOrigin variant', () => {
    const origins: ModelOrigin[] = [
      'default',
      'chat_override',
      'request_override',
      'learning_role',
      'verifier_role',
    ];
    for (const origin of origins) {
      const sent: TurnModelDto = {
        id: 'ornith', name: 'Ornith-1.5-9B',
        model: { kind: 'local', model_id: 'ornith-1.5-9b' },
        origin, provider: 'local', contextTokens: 8704, degraded: 'Metal buffer limit',
        seesImages: false,
      };
      expect(TurnModelSchema.parse(sent)).toEqual(sent);
    }
    const cloud: TurnModelDto = {
      id: 'claude',
      name: 'Claude',
      model: { kind: 'cloud', provider: 'anthropic', model: 'claude-x' },
      seesImages: false,
    };
    expect(TurnModelSchema.parse(cloud).model).toEqual(cloud.model);
  });

  it('strips future additive fields in persisted metadata schemas', () => {
    const model = TurnModelSchema.parse({
      id: 'qwen',
      name: 'Qwen3.5-27B',
      origin: 'verifier_role',
      futureModelField: 'ignored',
    });
    expect(model).toEqual({ id: 'qwen', name: 'Qwen3.5-27B', origin: 'verifier_role' });

    const step = TurnStepSchema.parse({
      id: 's1',
      kind: 'verify',
      label: 'Verifying claims',
      state: 'done',
      startedAtMs: 0,
      futureStepField: 123,
    });
    expect(step).toEqual({
      id: 's1',
      kind: 'verify',
      label: 'Verifying claims',
      state: 'done',
      startedAtMs: 0,
    });

    const trace = RetrievalTraceSchema.parse({
      searchedDocuments: 2,
      passages: 4,
      files: 2,
      scope: 'vault',
      futureTraceField: true,
    });
    expect(trace).toEqual({
      searchedDocuments: 2,
      passages: 4,
      files: 2,
      scope: 'vault',
    });

    const record = TurnRecordSchema.parse({
      model: { id: 'qwen', name: 'Qwen3.5-27B', origin: 'verifier_role', extra: 1 },
      steps: [],
      timing: {
        totalMs: 10,
        routerMs: 1,
        retrievalMs: 2,
        generationMs: 5,
        verificationMs: 2,
        toolMs: 0,
        futureTiming: 9,
      },
      tokens: { completion: 50, contextUsed: 200, futureTokens: 1 },
      router: null,
      futureRecordField: 'v2',
    });
    expect(record.model).toEqual({ id: 'qwen', name: 'Qwen3.5-27B', origin: 'verifier_role' });
  });

  it('refuses an origin or a reference it does not know', () => {
    expect(TurnModelSchema.safeParse({ id: 'a', name: 'a', origin: 'guess' }).success).toBe(false);
    expect(TurnModelSchema.safeParse({ id: 'a', name: 'a', model: { kind: 'local', path: '/x' } }).success).toBe(false);
  });
});
