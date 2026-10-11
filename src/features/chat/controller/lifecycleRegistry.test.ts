import { describe, expect, it, vi } from 'vitest';

import { createConversationLifecycleRegistry } from './lifecycleRegistry';

describe('conversation lifecycle registry', () => {
  it('releases completed operation callbacks instead of retaining deleted conversation ids', () => {
    const registry = createConversationLifecycleRegistry();
    const cleanup = vi.fn();

    for (let index = 0; index < 1000; index += 1) {
      const unregister = registry.register(`deleted-${index}`, cleanup);
      unregister();
    }

    registry.cleanupAll();
    expect(cleanup).not.toHaveBeenCalled();
  });
});
