import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useAppNavigate } from '@/hooks/useAppNavigate';

const h = vi.hoisted(() => ({ navigate: vi.fn(), capture: vi.fn() }));
vi.mock('react-router', () => ({ useNavigate: () => h.navigate }));
vi.mock('@/utils/diagnostics', () => ({ diagnostics: { capture: h.capture } }));

describe('useAppNavigate', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
  });

  it('preserves destinations, options, history deltas and callback identity', () => {
    const { result, rerender } = renderHook(() => useAppNavigate());
    const navigate = result.current;
    expect(navigate('/chat', { replace: true })).toBeUndefined();
    expect(h.navigate).toHaveBeenCalledWith('/chat', { replace: true });
    navigate(-1);
    expect(h.navigate).toHaveBeenLastCalledWith(-1);
    navigate('/journals');
    expect(h.navigate).toHaveBeenLastCalledWith('/journals');
    rerender();
    expect(result.current).toBe(navigate);
  });

  it('reports a rejected transition', async () => {
    const failure = new Error('route failed');
    h.navigate.mockRejectedValue(failure);
    const { result } = renderHook(() => useAppNavigate());
    await act(async () => { result.current('/chat'); });
    expect(h.capture).toHaveBeenCalledWith(failure, 'Navigation');
  });

  it('reports a synchronous router failure', () => {
    const failure = new Error('invalid history');
    h.navigate.mockImplementation(() => { throw failure; });
    const { result } = renderHook(() => useAppNavigate());
    expect(() => result.current(-1)).not.toThrow();
    expect(h.capture).toHaveBeenCalledWith(failure, 'Navigation');
  });
});
