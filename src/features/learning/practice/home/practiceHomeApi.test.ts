import { beforeEach, describe, expect, it, vi } from 'vitest';

import { isConflict } from '@/lib/api';

import { practiceHomeApi } from './practiceHomeApi';

const call = vi.hoisted(() => vi.fn());
vi.mock('@/lib/studyActivity', () => ({ studyActivityCall: call }));

beforeEach(() => call.mockReset());

describe('practiceHomeApi', () => {
  it('sends the request under the learning plugin and returns the data', async () => {
    call.mockResolvedValue({ ok: true, data: { items: [] } });
    await expect(practiceHomeApi.due('course-1')).resolves.toEqual({ items: [] });
    expect(call).toHaveBeenCalledWith('learning', 'practice_due', { request: { programId: 'course-1' } });
  });

  it('throws a failure that isConflict recognizes by the backend code', async () => {
    call.mockResolvedValue({ ok: false, error: 'changed', details: { code: 'CONFLICT' } });
    const error = await practiceHomeApi.getProfile('course-1').catch((cause: unknown) => cause);
    expect(error).toBeInstanceOf(Error);
    expect(isConflict(error)).toBe(true);
  });
});
