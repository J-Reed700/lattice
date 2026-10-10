import { describe, expect, it } from 'vitest';

import { jobFixture } from '@/tests/fixtures/jobs';

import { DEEP_RESEARCH_JOB, researchToRefresh } from './research';

const research = (status: 'running' | 'completed' | 'failed' | 'cancelled') =>
  jobFixture({ kind: DEEP_RESEARCH_JOB, subjectId: 'conversation-1', status, finishedAt: status === 'running' ? null : 2 });

describe('deep research jobs', () => {
  it('rereads a conversation whose research turn ended with no window waiting on it', () => {
    for (const status of ['completed', 'failed', 'cancelled'] as const) {
      expect(researchToRefresh(research(status), new Map())).toBe('conversation-1');
    }
  });

  it('leaves a turn this window is waiting on to its own request', () => {
    expect(researchToRefresh(research('completed'), new Map([['conversation-1', 'request-1']]))).toBeNull();
  });

  it('ignores research still running and every other kind of job', () => {
    expect(researchToRefresh(research('running'), new Map())).toBeNull();
    expect(researchToRefresh({ ...research('completed'), kind: 'journal.synthesis' }, new Map())).toBeNull();
  });
});
