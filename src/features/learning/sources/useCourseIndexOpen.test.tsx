import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useCourseIndexOpen } from '@/features/learning/sources/useCourseIndexStatus';

const api = vi.hoisted(() => ({ open: vi.fn(), close: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { courseIndexOpen: api.open, courseIndexClose: api.close } }));

describe('The course workspace puts its index first', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.open.mockResolvedValue({ ok: true, data: undefined });
    api.close.mockResolvedValue({ ok: true, data: undefined });
  });

  it('opens the course while mounted and closes it on leaving', () => {
    const { unmount } = renderHook(() => useCourseIndexOpen('program-1'));
    expect(api.open).toHaveBeenCalledExactlyOnceWith('program-1');
    expect(api.close).not.toHaveBeenCalled();
    unmount();
    expect(api.close).toHaveBeenCalledExactlyOnceWith('program-1');
  });

  it('moves the turn when another course opens in the same workspace', () => {
    const { rerender } = renderHook(({ id }) => useCourseIndexOpen(id), { initialProps: { id: 'program-1' } });
    rerender({ id: 'program-2' });
    expect(api.close).toHaveBeenCalledExactlyOnceWith('program-1');
    expect(api.open).toHaveBeenLastCalledWith('program-2');
  });

  it('asks nothing without a course', () => {
    renderHook(() => useCourseIndexOpen(null));
    expect(api.open).not.toHaveBeenCalled();
  });
});
