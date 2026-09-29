import { expect, it } from 'vitest';

import { RetrievalTraceSchema } from '../conversation';

it('keeps the trace and clips an over-long unavailable reason', () => {
  const parsed = RetrievalTraceSchema.safeParse({
    searchedDocuments: 0,
    passages: 0,
    files: 0,
    scope: 'vault',
    unavailableReason: 'x'.repeat(2000),
  });
  expect(parsed.success).toBe(true);
  expect(parsed.success && parsed.data.unavailableReason).toHaveLength(400);
});
