import { describe, expect, it } from 'vitest';

import type { TurnModelDto } from '@/lib/bindings';
import { TurnModelSchema } from '@/types/conversation';

describe('TurnModelSchema', () => {
  it('reads a record written before turns named their model', () => {
    expect(TurnModelSchema.parse({ id: 'qwen', name: 'Qwen3.5-27B' })).toEqual({ id: 'qwen', name: 'Qwen3.5-27B' });
  });

  it('reads every field the backend sends', () => {
    const sent: TurnModelDto = {
      id: 'ornith', name: 'Ornith-1.5-9B',
      model: { kind: 'local', model_id: 'ornith-1.5-9b' },
      origin: 'request_override', provider: 'local', contextTokens: 8704, degraded: 'Metal buffer limit',
      seesImages: false,
    };
    expect(TurnModelSchema.parse(sent)).toEqual(sent);
    const cloud: TurnModelDto = { ...sent, model: { kind: 'cloud', provider: 'anthropic', model: 'claude-x' } };
    expect(TurnModelSchema.parse(cloud).model).toEqual(cloud.model);
  });

  it('refuses an origin or a reference it does not know', () => {
    expect(TurnModelSchema.safeParse({ id: 'a', name: 'a', origin: 'guess' }).success).toBe(false);
    expect(TurnModelSchema.safeParse({ id: 'a', name: 'a', model: { kind: 'local', path: '/x' } }).success).toBe(false);
  });
});
