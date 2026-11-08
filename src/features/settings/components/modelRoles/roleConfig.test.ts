import { describe, expect, it } from 'vitest';

import { roleDescriptor } from '@/features/settings/components/modelRoles/roleConfig';

describe('role descriptors', () => {
  it('says what the Verifier audits in statements, not claims', () => {
    expect(roleDescriptor('verifier').hint).toBe('Audits every statement of a lesson against its cited passages before it publishes.');
  });
});
