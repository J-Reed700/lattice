import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ActivityEntryDto } from '@/lib/bindings';

import { useOpenActivity } from './useOpenActivity';

const mocks = vi.hoisted(() => ({
  navigate: vi.fn(),
  openDrawer: vi.fn(),
  setRoot: vi.fn(),
}));

vi.mock('@/hooks/useAppNavigate', () => ({
  useAppNavigate: () => mocks.navigate,
}));

vi.mock('@/features/model/hooks/useDownloadState', () => ({
  useDownloadState: () => ({ openDrawer: mocks.openDrawer }),
}));

vi.mock('@/features/explorer/stores/explorerStore', () => ({
  useExplorerStore: {
    getState: () => ({ setRoot: mocks.setRoot }),
  },
}));

function makeItem(
  overrides: Partial<ActivityEntryDto>,
): ActivityEntryDto {
  return {
    id: 'activity-1',
    kind: 'learning.outline',
    title: 'Outlining a new course',
    state: 'running',
    detail: null,
    progress: null,
    perSecond: null,
    secondsLeft: null,
    holding: null,
    target: null,
    since: null,
    retryAt: null,
    ...overrides,
  };
}

describe('useOpenActivity', () => {
  beforeEach(() => {
    mocks.navigate.mockReset();
    mocks.openDrawer.mockReset();
    mocks.setRoot.mockReset();
  });

  it('navigates to Studio builder when target is studio_builder', () => {
    const onOpened = vi.fn();
    const { result } = renderHook(() => useOpenActivity(onOpened));

    act(() => {
      result.current(makeItem({ target: { type: 'studio_builder' } }));
    });

    expect(mocks.navigate).toHaveBeenCalledWith('/studio?builder=1');
    expect(onOpened).toHaveBeenCalledTimes(1);
  });

  it('navigates to course, lesson, folder, and conversation targets', () => {
    const onOpened = vi.fn();
    const { result } = renderHook(() => useOpenActivity(onOpened));

    act(() => {
      result.current(makeItem({ target: { type: 'course', course: 'c1' } }));
      result.current(makeItem({ target: { type: 'lesson', course: 'c1', lesson: 'l1' } }));
      result.current(makeItem({ target: { type: 'folder', root: '/tmp/docs', name: 'Docs' } }));
      result.current(makeItem({ target: { type: 'conversation', conversation: 'conv-1' } }));
    });

    expect(mocks.navigate).toHaveBeenNthCalledWith(1, '/studio?course=c1');
    expect(mocks.navigate).toHaveBeenNthCalledWith(2, '/studio?course=c1&lesson=l1');
    expect(mocks.setRoot).toHaveBeenCalledWith({ root: '/tmp/docs', name: 'Docs' });
    expect(mocks.navigate).toHaveBeenNthCalledWith(3, '/explorer');
    expect(mocks.navigate).toHaveBeenNthCalledWith(4, '/chat?conversationId=conv-1');
    expect(onOpened).toHaveBeenCalledTimes(4);
  });
});
