import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useMessageEvidence } from '@/features/chat/hooks/useMessageEvidence';
import type { ClaimVerdict, SourceWithMetadata } from '@/types/conversation';

const h = vi.hoisted(() => ({
  reader: { open: vi.fn(), session: null as null | Record<string, unknown> },
  settings: { vault: { vaultPath: '/Users/ada/Lattice' } } as unknown,
  references: [] as Array<{ documentId: string; chunkId?: string }>,
  latest: null as null | ReturnType<typeof useMessageEvidence>,
}));

vi.mock('@/features/reading/stores/chatReaderStore', () => ({
  useChatReaderStore: (selector: (state: unknown) => unknown) => selector(h.reader),
}));
vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({ useSettingsQuery: () => ({ data: h.settings }) }));
vi.mock('@/features/reading/hooks/usePassageReferencesQuery', () => ({ usePassageReferenceIds: () => h.references }));

const source = (n: number, filePath = `/library/source-${n}.pdf`) => ({
  documentId: `d${n}`, chunkId: `d${n}#c`, fileName: `Source ${n}`, filePath, citationId: n,
}) as unknown as SourceWithMetadata;

const sources = [source(1), source(2, '/Users/ada/Lattice/notes/2026-05-01.md'), source(3)];
const verdicts = [{ citationIds: [1, 3] }, { citationIds: [] }] as unknown as ClaimVerdict[];

type Options = Partial<Parameters<typeof useMessageEvidence>[0]>;

function Harness(options: Options) {
  const evidence = useMessageEvidence({
    sources,
    ownerKey: 'answer:m2',
    isAssistantWithSources: true,
    showCitations: true,
    claimVerdicts: verdicts,
    showsCodeRefs: true,
    revealInExplorer: options.revealInExplorer ?? vi.fn(),
    openInExplorer: options.openInExplorer ?? vi.fn(),
    ...options,
  });
  h.latest = evidence;
  return (
    <article ref={evidence.articleRef as never}>
      <div
        data-testid="body"
        onClick={evidence.handleBodyClick}
        onMouseOver={evidence.handleBodyMouseOver}
        onMouseLeave={evidence.handleBodyMouseLeave}
      >
        <span data-claim="0">
          Rome traded silk <sup data-cite="1" data-cite-at="0">[1]</sup> and spice <sup data-cite="3">[3]</sup>.
        </span>
        <span data-claim="1">An unsupported line.</span>
        <span data-claim="7">A claim with no verdict.</span>
        <sup data-cite="9">[9]</sup>
        <a href="/notes/rome.md" data-code-ref="notes/rome.md" data-code-ref-start="3" data-code-ref-end="9">rome.md:3-9</a>
        <a href="/notes/carthage.md" data-code-ref="notes/carthage.md">carthage.md</a>
        <a href="#" data-code-ref="">empty</a>
        <p>Plain text</p>
      </div>
    </article>
  );
}

const ev = () => h.latest!;

describe('useMessageEvidence', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    h.reader.open.mockClear();
    h.reader.session = null;
    h.references = [{ documentId: 'd3', chunkId: 'd3#c' }];
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('numbers the sources and labels where they came from', () => {
    render(<Harness />);
    expect(ev().citationNumbers).toEqual([1, 2, 3]);
    expect(ev().provenanceBySource.get('d2#c')).toBe('from your journal');
    expect(ev().provenanceBySource.get('d3#c')).toBe('from your references');
    expect(ev().provenanceBySource.has('d1#c')).toBe(false);
  });

  it('lists no numbers for an answer without sources, and copes with no settings', () => {
    h.settings = undefined;
    render(<Harness isAssistantWithSources={false} />);
    expect(ev().citationNumbers).toEqual([]);
    h.settings = { vault: { vaultPath: '/Users/ada/Lattice' } };
  });

  it('opens the reader on a citation, at the mark that was clicked', () => {
    render(<Harness />);
    fireEvent.click(screen.getByText('[1]'));
    expect(h.reader.open).toHaveBeenCalledWith('answer:m2', sources, 0, 0);
    fireEvent.click(screen.getByText('[3]'));
    expect(h.reader.open).toHaveBeenLastCalledWith('answer:m2', sources, 2, null);
    fireEvent.click(screen.getByText('[9]'));
    expect(h.reader.open).toHaveBeenCalledTimes(2);
  });

  it('opens a passage the answer never numbered on its own', () => {
    render(<Harness />);
    const stray = { ...source(4), chunkId: 'stray' } as SourceWithMetadata;
    act(() => ev().openSource(stray, 2));
    expect(h.reader.open).toHaveBeenCalledWith('answer:m2', [stray], 0, 2);
    act(() => ev().openCitation(-1));
    expect(h.reader.open).toHaveBeenCalledTimes(1);
  });

  it('opens nothing without an owner', () => {
    render(<Harness ownerKey="" />);
    act(() => ev().openSource({ ...source(4), chunkId: 'stray' } as SourceWithMetadata));
    act(() => ev().openCitation(0));
    expect(h.reader.open).not.toHaveBeenCalled();
  });

  it('opens code references in the Explorer, at their lines when it has them', () => {
    const revealInExplorer = vi.fn();
    const openInExplorer = vi.fn();
    render(<Harness revealInExplorer={revealInExplorer} openInExplorer={openInExplorer} />);
    fireEvent.click(screen.getByText('rome.md:3-9'));
    expect(revealInExplorer).toHaveBeenCalledWith('notes/rome.md', { startLine: 3, endLine: 9 });
    fireEvent.click(screen.getByText('carthage.md'));
    expect(openInExplorer).toHaveBeenCalledWith('notes/carthage.md');
    fireEvent.click(screen.getByText('empty'));
    expect(openInExplorer).toHaveBeenCalledTimes(1);
  });

  it('ignores code references where they are not shown, and citations when hidden', () => {
    const openInExplorer = vi.fn();
    render(<Harness showsCodeRefs={false} showCitations={false} openInExplorer={openInExplorer} />);
    fireEvent.click(screen.getByText('carthage.md'));
    fireEvent.click(screen.getByText('[1]'));
    expect(openInExplorer).not.toHaveBeenCalled();
    expect(h.reader.open).not.toHaveBeenCalled();
  });

  describe('sentences', () => {
    it('offers actions for a clicked sentence with a verdict', () => {
      render(<Harness />);
      fireEvent.click(screen.getByText('An unsupported line.'));
      expect(ev().claimActions?.index).toBe(1);
      act(() => ev().setClaimActions(null));
      fireEvent.click(screen.getByText('A claim with no verdict.'));
      fireEvent.click(screen.getByText('Plain text'));
      expect(ev().claimActions).toBeNull();
    });

    it('treats a click that ends a selection as a selection', () => {
      vi.spyOn(window, 'getSelection').mockReturnValue({ toString: () => 'Rome traded' } as Selection);
      render(<Harness />);
      fireEvent.click(screen.getByText('An unsupported line.'));
      expect(ev().claimActions).toBeNull();
    });

    it('lights a hovered sentence and opens its card after a pause, then follows the pointer at once', () => {
      render(<Harness />);
      fireEvent.mouseOver(screen.getByText('An unsupported line.'));
      expect(screen.getByText('An unsupported line.')).toHaveClass('is-lit');
      expect(ev().claimHover).toBeNull();
      act(() => { vi.advanceTimersByTime(280); });
      expect(ev().claimHover?.index).toBe(1);
      expect(ev().hoveredVerdict).toBe(verdicts[1]);
      fireEvent.mouseOver(screen.getByText('An unsupported line.'));
      fireEvent.mouseOver(screen.getByText(/Rome traded silk/));
      expect(ev().claimHover?.index).toBe(0);
      expect(ev().activeEvidence).toEqual([1, 3]);
      fireEvent.mouseOver(screen.getByText('Plain text'));
      expect(ev().claimHover).toBeNull();
      expect(document.querySelectorAll('.is-lit')).toHaveLength(0);
    });

    it('shows a citation card on a chip, and clears everything on leave', () => {
      render(<Harness />);
      fireEvent.mouseOver(screen.getByText('An unsupported line.'));
      act(() => { vi.advanceTimersByTime(280); });
      fireEvent.mouseOver(screen.getByText('[3]'));
      expect(ev().citationHover?.number).toBe(3);
      expect(ev().hoveredSource).toBe(sources[2]);
      expect(ev().claimHover).toBeNull();
      expect(ev().activeEvidence).toEqual([3]);
      fireEvent.mouseOver(screen.getByText('[3]'));
      fireEvent.mouseOver(screen.getByText('Plain text'));
      expect(ev().citationHover).toBeNull();
      fireEvent.mouseOver(screen.getByText('[1]'));
      fireEvent.mouseLeave(screen.getByTestId('body'));
      expect(ev().citationHover).toBeNull();
    });

    it('drops every card when citations are hidden', () => {
      const { rerender } = render(<Harness />);
      fireEvent.mouseOver(screen.getByText('[1]'));
      fireEvent.click(screen.getByText('An unsupported line.'));
      rerender(<Harness showCitations={false} />);
      expect(ev().citationHover).toBeNull();
      expect(ev().claimActions).toBeNull();
    });
  });

  describe('the open citation', () => {
    it('stays lit while the reader shows it: one mark, or the whole source', () => {
      h.reader.session = { ownerKey: 'answer:m2', citations: sources, index: 0, occurrence: 0 };
      const { rerender } = render(<Harness />);
      expect(screen.getByText('[1]')).toHaveClass('is-lit');
      h.reader.session = { ownerKey: 'answer:m2', citations: sources, index: 2, occurrence: null };
      rerender(<Harness />);
      expect(screen.getByText('[3]')).toHaveClass('is-lit');
      expect(screen.getByText('[1]')).not.toHaveClass('is-lit');
    });

    it('lights nothing for another answer or a passage it does not cite', () => {
      h.reader.session = { ownerKey: 'answer:m9', citations: sources, index: 0, occurrence: null };
      const { rerender } = render(<Harness />);
      expect(document.querySelectorAll('.is-lit')).toHaveLength(0);
      h.reader.session = { ownerKey: 'answer:m2', citations: [{ chunkId: 'elsewhere' }], index: 0, occurrence: null };
      rerender(<Harness />);
      h.reader.session = { ownerKey: 'answer:m2', citations: [], index: 4, occurrence: null };
      rerender(<Harness />);
      expect(document.querySelectorAll('.is-lit')).toHaveLength(0);
    });
  });

  it('knows whether the evidence margin is showing', () => {
    render(<Harness />);
    expect(ev().hasEvidenceMargin()).toBe(false);
  });
});
