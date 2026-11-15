import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useRevealStreamingRef } from '@/features/chat/hooks/useRevealStreamingRef';
import type { FileTarget } from '@/utils/replyPieces';

const h = vi.hoisted(() => ({ reveal: vi.fn() }));
vi.mock('@/features/chat/model/folderThreadHost', () => ({ revealInFolder: h.reveal }));

const rome: FileTarget = { name: 'notes/rome.md', lines: { first: 12, last: 12 } };
const carthage: FileTarget = { name: 'notes/carthage.md', lines: { first: 3, last: 8 } };

describe('useRevealStreamingRef', () => {
  beforeEach(() => {
    h.reveal.mockClear();
  });

  it('shows the newest file reference as it streams, once each', () => {
    const { rerender } = renderHook(({ files, active }) => useRevealStreamingRef(files, active), {
      initialProps: { files: [rome], active: true },
    });
    expect(h.reveal).toHaveBeenCalledWith('notes/rome.md', { startLine: 12, endLine: 12 });
    rerender({ files: [rome], active: true });
    expect(h.reveal).toHaveBeenCalledTimes(1);
    rerender({ files: [rome, carthage], active: true });
    expect(h.reveal).toHaveBeenLastCalledWith('notes/carthage.md', { startLine: 3, endLine: 8 });
  });

  it('takes a reference that arrives with the end of streaming', () => {
    const { rerender } = renderHook(({ files, active }) => useRevealStreamingRef(files, active), {
      initialProps: { files: [] as FileTarget[], active: true },
    });
    rerender({ files: [rome], active: false });
    expect(h.reveal).toHaveBeenCalledWith('notes/rome.md', { startLine: 12, endLine: 12 });
  });

  it('does nothing while inactive, without files, or for a file without lines', () => {
    const { rerender } = renderHook(({ files, active }) => useRevealStreamingRef(files, active), {
      initialProps: { files: [rome], active: false },
    });
    rerender({ files: [rome, carthage], active: false });
    rerender({ files: [], active: true });
    rerender({ files: [{ name: 'README.md', lines: null }], active: true });
    expect(h.reveal).not.toHaveBeenCalled();
  });
});
