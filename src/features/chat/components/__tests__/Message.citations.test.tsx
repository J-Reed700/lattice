import { fireEvent, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { Message } from '@/features/chat/components/Message';
import { citationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import type { ReplyPieceDto } from '@/lib/bindings';
import { render } from '@/tests/renderWithQueryClient';
import type { DisplayMessage, SourceWithMetadata } from '@/types/conversation';
import { withoutMarks } from '@/utils/replyPieces';

vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (selector: (_state: unknown) => unknown) =>
    selector({
      messageVerification: new Map(),
      messageBookmarkMap: new Map(),
      lastMessageSources: new Map(),
      messageRetrieval: new Map(),
      liveRetrieval: new Map(),
      liveSteps: new Map(),
      liveModel: new Map(),
      messageTurn: new Map(),
      inFlightGenerations: new Map(),
      conversations: [{ id: 'conversation', explorerRoot: null }],
      regenerateResponse: vi.fn(),
      sendMessage: vi.fn(),
      retryFailedMessage: vi.fn(),
      dismissFailedMessage: vi.fn(),
    }),
}));
vi.mock('react-router', async () => {
  const actual = await vi.importActual<typeof import('react-router')>('react-router');
  return { ...actual, useNavigate: () => vi.fn() };
});
vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => ({ data: undefined }),
  useUpdateSettingsMutation: () => ({ mutateAsync: vi.fn() }),
}));
vi.mock('@/features/reading/hooks/usePassageReferencesQuery', () => ({ usePassageReferenceIds: () => [] }));
vi.mock('@/features/model/hooks/useActiveModels', () => ({ useActiveModels: () => ({ activeModels: null }) }));
vi.mock('@/features/model/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => ({ downloadedModels: [] }) }));
vi.mock('@/features/model/hooks/useLoadedModels', () => ({ useLoadedModels: () => [], degradedReasonFor: () => null }));
vi.mock('@/features/chat/components/MessageActions', () => ({ MessageActions: () => null }));
vi.mock('@/features/chat/components/turn/TurnRecord', () => ({ TurnRecord: () => null }));

describe('Message anchored citations and Quoted pieces', () => {
  beforeEach(() => {
    citationDisplayStore.getState().setVisible(true);
    useReaderStore.setState({ session: null, resolvedLocations: new Map() });
  });

  it('draws Quoted prose, shows per-occurrence CitationCard hover details, and opens anchored reader sessions', async () => {
    const snapshotText = [
      '# Operations Manual',
      '## Hold Timer',
      'Hold the mitigation for 20 seconds.',
      '## Escalation',
      'Once the hold expires, page Marisol Quenby.',
    ].join('\n\n');

    const sources: SourceWithMetadata[] = [
      {
        documentId: 'web:https://example.org/manual',
        chunkId: 'web:https://example.org/manual#0',
        fileName: 'Operations Manual',
        filePath: 'https://example.org/manual',
        mimeType: 'text/html',
        category: 'Web article',
        content: 'Hold the mitigation for 20 seconds.',
        webSnapshot: {
          url: 'https://example.org/manual',
          title: 'Operations Manual',
          text: snapshotText,
          fetchedAt: null,
          truncated: false,
        },
        score: 0.9,
        fileSizeBytes: 0,
        modifiedAt: '2026-10-20T00:00:00.000Z',
        citationId: 1,
      },
    ];

    const reply: ReplyPieceDto[] = [
      { type: 'quoted', text: 'Hold the mitigation for 20 seconds' },
      { type: 'text', text: ' ' },
      {
        type: 'reference',
        reference: {
          written: '[s1]',
          kind: 'source',
          number: 1,
          support: { relation: 'quotes', anchors: [{ exact: 'Hold the mitigation for 20 seconds.' }] },
        },
      },
      { type: 'text', text: '. Notify Marisol afterwards ' },
      {
        type: 'reference',
        reference: {
          written: '[s1]',
          kind: 'source',
          number: 1,
          support: { relation: 'backs', anchors: [{ exact: 'Once the hold expires, page Marisol Quenby.' }] },
        },
      },
      { type: 'text', text: '.' },
    ];

    const message: DisplayMessage = {
      id: 'msg-anchored-chat',
      conversationId: 'conversation',
      role: 'assistant',
      content: 'Hold the mitigation for 20 seconds [s1]. Notify Marisol afterwards [s1].',
      tokens: 20,
      status: 'completed',
      createdAt: '2026-10-20T00:00:00.000Z',
      sources,
      reply,
    };

    const { container } = render(
      <Message message={message} isLastTurn={false} />,
    );

    const quotes = Array.from(container.querySelectorAll('.cite-quote')).map((el) => el.textContent);
    expect(quotes).toEqual(['“', '”']);
    expect(withoutMarks(container.textContent ?? '')).toContain('“Hold the mitigation for 20 seconds”');

    const chips = await screen.findAllByRole('button', { name: 'Citation 1' });
    expect(chips).toHaveLength(2);

    fireEvent.mouseOver(chips[0]!);
    let tooltip = await screen.findByRole('tooltip');
    expect(tooltip).toHaveTextContent('Quoted from');
    expect(tooltip).toHaveTextContent('“Hold the mitigation for 20 seconds.”');
    expect(tooltip).toHaveTextContent('Operations Manual › Hold Timer');

    fireEvent.mouseOver(chips[1]!);
    tooltip = await screen.findByRole('tooltip');
    expect(tooltip).toHaveTextContent('Supported by');
    expect(tooltip).toHaveTextContent('Once the hold expires, page Marisol Quenby.');
    expect(tooltip).toHaveTextContent('Operations Manual › Escalation');

    fireEvent.click(chips[1]!);
    const session = useReaderStore.getState().session;
    expect(session?.occurrence).toBe(1);
    expect(session?.citations[0]?.anchoredBy).toEqual([
      { occurrence: 0, relation: 'quotes', anchors: [{ exact: 'Hold the mitigation for 20 seconds.' }] },
      { occurrence: 1, relation: 'backs', anchors: [{ exact: 'Once the hold expires, page Marisol Quenby.' }] },
    ]);
  });
});
