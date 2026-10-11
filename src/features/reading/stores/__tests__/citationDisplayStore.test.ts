import { beforeEach, describe, expect, it } from 'vitest';

import {
  CITATION_DISPLAY_STORAGE_KEY,
  useCitationDisplayStore,
} from '@/features/reading/stores/citationDisplayStore';

describe('citation display preference', () => {
  beforeEach(() => {
    localStorage.clear();
    useCitationDisplayStore.setState({ visible: true });
  });

  it('persists clean reading without deleting evidence state', () => {
    useCitationDisplayStore.getState().setVisible(false);
    expect(useCitationDisplayStore.getState().visible).toBe(false);
    expect(localStorage.getItem(CITATION_DISPLAY_STORAGE_KEY)).toBe('0');

    useCitationDisplayStore.getState().toggle();
    expect(useCitationDisplayStore.getState().visible).toBe(true);
    expect(localStorage.getItem(CITATION_DISPLAY_STORAGE_KEY)).toBe('1');
  });
});
