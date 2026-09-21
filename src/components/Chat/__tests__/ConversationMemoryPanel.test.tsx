import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ConversationMemoryPanel } from '../ConversationMemoryPanel';

import type {
  ConversationMemoryDetailsDto,
  ConversationMemoryItemDto,
  MemoryEvidenceDto,
} from '../../../lib/bindings';

const getConversationMemory = vi.hoisted(() => vi.fn());

vi.mock('../../../lib/api', () => ({
  VaultAPI: { getConversationMemory },
}));

const quote = (overrides: Partial<MemoryEvidenceDto> = {}): MemoryEvidenceDto => ({
  messageId: 'm1',
  sequence: 3,
  role: 'user',
  purpose: 'assertion',
  startByte: 0,
  endByte: 10,
  text: 'Keep it under 2000 dollars.',
  ...overrides,
});

const item = (overrides: Partial<ConversationMemoryItemDto> = {}): ConversationMemoryItemDto => ({
  id: 'item-1',
  kind: 'constraint',
  state: 'active',
  review: 'supported',
  label: 'Budget is capped at $2,000.',
  isMandatory: true,
  createdAtSequence: 3,
  changedAtSequence: 3,
  supersededBy: null,
  relatedItemIds: [],
  evidence: [quote()],
  ...overrides,
});

const details = (
  overrides: Partial<ConversationMemoryDetailsDto> = {}
): ConversationMemoryDetailsDto => ({
  conversationId: 'conv-1',
  mode: 'ready',
  schemaVersion: 1,
  memoryRevision: 4,
  transcriptRevision: 9,
  processedThroughSequence: 12,
  activeMandatoryCount: 1,
  activeOptionalCount: 2,
  conflictCount: 0,
  summary: 'Planning a trip on a tight budget.',
  items: [item()],
  history: [],
  lastErrorCode: null,
  extractorModelIdentity: null,
  featureEnabled: true,
  ...overrides,
});

function resolveWith(value: ConversationMemoryDetailsDto) {
  getConversationMemory.mockResolvedValue({ ok: true, data: value });
}

function renderPanel(props: { onClose?: () => void; messageContentById?: Map<string, string> } = {}) {
  const onClose = props.onClose ?? vi.fn();
  const view = render(
    <ConversationMemoryPanel
      conversationId="conv-1"
      isOpen
      onClose={onClose}
      messageContentById={props.messageContentById}
    />
  );
  return { ...view, onClose };
}

/**
 * Stand in for the rendered thread behind the panel. The jump finds its target
 * by DOM id, so a quotation can only lead somewhere if that message is on
 * screen.
 */
const threadNodes: HTMLElement[] = [];

function mountThreadMessage(messageId: string, content: string) {
  const host = document.createElement('div');
  host.id = `message-${messageId}`;
  const body = document.createElement('p');
  body.textContent = content;
  host.append(body);
  document.body.append(host);
  threadNodes.push(host);
  return host;
}

function selectedText(): string {
  return window.getSelection()?.toString() ?? '';
}

describe('ConversationMemoryPanel', () => {
  beforeEach(() => {
    getConversationMemory.mockReset();
    // jsdom has no layout, so scrolling is a no-op it does not implement.
    Element.prototype.scrollIntoView = vi.fn();
    window.getSelection()?.removeAllRanges();
  });

  afterEach(() => {
    while (threadNodes.length > 0) threadNodes.pop()?.remove();
  });

  it('marks a quotation as unavailable instead of hiding the item when its source message was edited or deleted', async () => {
    // A hidden row would silently turn an item with no surviving source into
    // one that looks fully sourced.
    resolveWith(details({ items: [item({ evidence: [quote({ text: null })] })] }));
    renderPanel();

    await waitFor(() => {
      expect(screen.getByTestId('memory-evidence-missing')).toBeInTheDocument();
    });
    expect(screen.getByText(/source no longer available/i)).toBeInTheDocument();
    expect(screen.getByTestId('memory-item-label')).toHaveTextContent(
      'Budget is capped at $2,000.'
    );
    expect(screen.queryByTestId('memory-evidence-quote')).not.toBeInTheDocument();
  });

  it('renders the generated label and the quoted source as visually different things', async () => {
    // If they share a type treatment, a model-written sentence reads as the
    // user's own words.
    resolveWith(details());
    renderPanel();

    const label = await screen.findByTestId('memory-item-label');
    const quoted = screen.getByTestId('memory-evidence-quote');
    expect(label.className).not.toEqual(quoted.className);
    expect(label.className).toContain('font-medium');
    expect(label.className).toContain('not-italic');
    // The quotation carries a rule and italic serif; the label carries neither.
    expect(quoted.querySelector('blockquote')?.className).toContain('border-l-2');
    expect(quoted.querySelector('p')?.className).toContain('italic');
    expect(quoted.querySelector('p')?.className).toContain('font-serif');
  });

  it('tells the user rebuilding is temporary rather than describing it as working memory', async () => {
    resolveWith(details({ mode: 'rebuild_required' }));
    renderPanel();

    const status = await screen.findByTestId('memory-status');
    expect(status).toHaveAttribute('data-tone', 'waiting');
    expect(status).toHaveTextContent(/rebuilding/i);
    expect(status).toHaveTextContent(/temporary/i);
    expect(status).toHaveTextContent(/not being added to prompts/i);
  });

  it('tells the user an unsupported schema needs the app updated, not that it will fix itself', async () => {
    resolveWith(details({ mode: 'unsupported_schema' }));
    renderPanel();

    const status = await screen.findByTestId('memory-status');
    expect(status).toHaveAttribute('data-tone', 'blocked');
    expect(status).toHaveTextContent(/update the app/i);
    expect(status).not.toHaveTextContent(/temporary/i);
  });

  it('describes ready memory as in use and distinct from the other two modes', async () => {
    resolveWith(details());
    renderPanel();

    const status = await screen.findByTestId('memory-status');
    expect(status).toHaveAttribute('data-tone', 'active');
    expect(status).toHaveTextContent(/every prompt/i);
    expect(status).not.toHaveTextContent(/rebuilding/i);
    expect(status).not.toHaveTextContent(/update the app/i);
  });

  it('never advertises active memory while the rollout switch is off, even when the store is ready', async () => {
    resolveWith(details({ featureEnabled: false, mode: 'ready' }));
    renderPanel();

    const status = await screen.findByTestId('memory-status');
    expect(status).toHaveAttribute('data-tone', 'off');
    expect(status).toHaveTextContent(/off/i);
    expect(status).toHaveTextContent(/nothing below is being sent to the model/i);
    expect(status).not.toHaveTextContent(/in use/i);
  });

  it('labels the summary as generated so it is not read as a transcript', async () => {
    resolveWith(details());
    renderPanel();

    expect(await screen.findByText(/generated, and can be wrong/i)).toBeInTheDocument();
    expect(screen.getByText('Planning a trip on a tight budget.')).toBeInTheDocument();
  });

  it('surfaces the counts, the processed sequence and the last error code', async () => {
    resolveWith(details({ conflictCount: 2, lastErrorCode: 'extractor_timeout' }));
    renderPanel();

    expect(await screen.findByText(/required \(every prompt\)/i)).toBeInTheDocument();
    expect(screen.getByText(/optional \(when relevant\)/i)).toBeInTheDocument();
    expect(screen.getByText(/needs your attention/i)).toBeInTheDocument();
    expect(screen.getByText('12')).toBeInTheDocument();
    expect(screen.getByText('extractor_timeout')).toBeInTheDocument();
  });

  it('marks which items ride in every prompt', async () => {
    resolveWith(details({ items: [item(), item({ id: 'item-2', isMandatory: false })] }));
    renderPanel();

    await waitFor(() => expect(screen.getAllByTestId('memory-item')).toHaveLength(2));
    expect(screen.getByText('In every prompt')).toBeInTheDocument();
    expect(screen.getByText('Only when relevant')).toBeInTheDocument();
  });

  it('asks the backend for superseded items only once the user requests them', async () => {
    resolveWith(details());
    renderPanel();

    await waitFor(() => expect(getConversationMemory).toHaveBeenCalledTimes(1));
    expect(getConversationMemory).toHaveBeenLastCalledWith('conv-1', false);

    resolveWith(
      details({ history: [item({ id: 'old-1', state: 'superseded', label: 'Budget was $1,000.' })] })
    );
    await userEvent.click(screen.getByRole('button', { name: /show earlier versions/i }));

    await waitFor(() => expect(getConversationMemory).toHaveBeenLastCalledWith('conv-1', true));
    expect(await screen.findByText('Budget was $1,000.')).toBeInTheDocument();
  });

  it('takes the reader to the message a surviving quotation came from', async () => {
    const content = 'Keep it under 2000 dollars.';
    const target = mountThreadMessage('m1', content);
    resolveWith(details({ items: [item({ evidence: [quote({ text: content, startByte: 0, endByte: 27 })] })] }));
    const { onClose } = renderPanel({ messageContentById: new Map([['m1', content]]) });

    const open = await screen.findByTestId('memory-evidence-open-source');
    expect(open).toHaveAttribute(
      'aria-label',
      'Open message 3 in the conversation and highlight this quotation'
    );
    await userEvent.click(open);

    // The panel steps aside so the thread it scrolled is the thing on screen.
    expect(onClose).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(target.scrollIntoView).toHaveBeenCalled());
  });

  it('marks the validated span by its UTF-8 byte offsets, not by slicing the string', async () => {
    // 'Café —' is 10 bytes but 7 characters, so a plain slice(10, 43) would mark
    // 'p the total under 2000 dollars.' — three characters adrift, and silently.
    const content = 'Café — keep the total under 2000 dollars.';
    const validated = 'keep the total under 2000 dollars';
    mountThreadMessage('m1', content);
    resolveWith(
      details({
        items: [
          item({ evidence: [quote({ text: validated, startByte: 10, endByte: 43 })] }),
        ],
      })
    );
    renderPanel({ messageContentById: new Map([['m1', content]]) });

    await userEvent.click(await screen.findByTestId('memory-evidence-open-source'));

    await waitFor(() => expect(selectedText()).toBe(validated));
    expect(selectedText()).not.toBe(content.slice(10, 43));
  });

  it('leaves the span unmarked rather than guessing when the stored message no longer reproduces the quotation', async () => {
    const content = 'Keep it under 2000 dollars.';
    const target = mountThreadMessage('m1', content);
    resolveWith(
      details({
        items: [item({ evidence: [quote({ text: 'a budget that was never said', startByte: 0, endByte: 10 })] })],
      })
    );
    renderPanel({ messageContentById: new Map([['m1', content]]) });

    await userEvent.click(await screen.findByTestId('memory-evidence-open-source'));

    await waitFor(() => expect(target.scrollIntoView).toHaveBeenCalled());
    expect(selectedText()).toBe('');
  });

  it('offers no way back to a message whose quotation no longer resolves', async () => {
    // Its source is gone; a control leading to a message that may have been
    // deleted would promise something the panel cannot honour.
    mountThreadMessage('m1', 'Keep it under 2000 dollars.');
    resolveWith(details({ items: [item({ evidence: [quote({ text: null })] })] }));
    renderPanel();

    await screen.findByTestId('memory-evidence-missing');
    expect(screen.queryByTestId('memory-evidence-open-source')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /open message/i })).not.toBeInTheDocument();
  });

  it('says the message could not be found instead of closing onto a thread that does not hold it', async () => {
    // Nothing is mounted for m1: the jump would scroll nowhere, so the panel
    // stays open and explains itself where the reader clicked.
    resolveWith(details());
    const { onClose } = renderPanel({
      messageContentById: new Map([['m1', 'Keep it under 2000 dollars.']]),
    });

    await userEvent.click(await screen.findByTestId('memory-evidence-open-source'));

    expect(await screen.findByTestId('memory-evidence-unreachable')).toHaveTextContent(
      /couldn't find message 3/i
    );
    expect(onClose).not.toHaveBeenCalled();
  });

  it('offers no way to edit or add a memory item', async () => {
    // Read-only is the guarantee: an editable item would be a requirement with
    // no source behind it.
    resolveWith(details());
    renderPanel();

    await screen.findByTestId('memory-item');
    expect(screen.queryAllByRole('textbox')).toHaveLength(0);
    for (const label of [/edit/i, /add/i, /save/i, /delete/i, /remove/i]) {
      expect(screen.queryByRole('button', { name: label })).not.toBeInTheDocument();
    }
  });
});
