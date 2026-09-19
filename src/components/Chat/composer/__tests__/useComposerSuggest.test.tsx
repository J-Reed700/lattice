import { useRef, useState } from 'react';

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { ComposerSuggest } from '../ComposerSuggest';
import { matchSlashCommands } from '../slashCommands';
import { replaceTrigger } from '../suggestTrigger';
import { useComposerSuggest } from '../useComposerSuggest';

import type { ComposerModeState } from '../slashCommands';

/**
 * The composer's keyboard contract, on a stand-in for the real one: a textarea
 * that sends on Enter, with the popup wired the way `ChatPanel` wires it.
 */

const MODE: ComposerModeState = {
  turnMode: 'auto',
  knowledgeBase: false,
  webSearch: false,
  wikipedia: false,
  deepResearch: false,
  canCompact: true,
};

interface HarnessProps {
  onSend: (_value: string) => void;
  onRun: (_id: string) => void;
}

function Harness({ onSend, onRun }: HarnessProps) {
  const [value, setValue] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const suggest = useComposerSuggest({
    value,
    textareaRef,
    resolveItems: (trigger) =>
      trigger.kind === 'slash'
        ? matchSlashCommands(trigger.query, MODE).map((command) => ({
            id: command.id,
            label: `/${command.token}`,
            description: command.description,
            state: command.state(MODE),
            icon: command.icon,
          }))
        : [],
    onAccept: (item, trigger) => {
      setValue(replaceTrigger(value, trigger, '').value);
      onRun(item.id);
    },
  });

  return (
    <div className="composer-caret-field">
      <textarea
        ref={textareaRef}
        aria-label="Message composer"
        value={value}
        onChange={(event) => {
          setValue(event.target.value);
          suggest.syncCaret(event.target);
        }}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return;
          if (suggest.handleKeyDown(event)) return;
          if (event.key === 'Enter' && !event.shiftKey) {
            event.preventDefault();
            onSend(value);
          }
        }}
      />
      {suggest.isOpen && suggest.point && (
        <ComposerSuggest
          items={suggest.items}
          activeIndex={suggest.activeIndex}
          heading="Commands"
          emptyLabel="No command matches."
          point={suggest.point}
          listId="test-suggest"
          onSelect={suggest.accept}
          onHover={suggest.setActiveIndex}
        />
      )}
    </div>
  );
}

const renderComposer = () => {
  const onSend = vi.fn();
  const onRun = vi.fn();
  render(<Harness onSend={onSend} onRun={onRun} />);
  return { onSend, onRun, textarea: screen.getByLabelText('Message composer') };
};

describe('the composer suggestion popup', () => {
  it('opens on a slash and narrows as the command is typed', async () => {
    const { textarea } = renderComposer();
    await userEvent.type(textarea, '/');
    expect(screen.getAllByRole('option').length).toBeGreaterThan(4);

    await userEvent.type(textarea, 'w');
    const rows = screen.getAllByRole('option');
    // What the name starts with first, then anything else the name contains.
    expect(rows).toHaveLength(3);
    expect(rows[0]).toHaveTextContent('/web');
    expect(rows[1]).toHaveTextContent('/wiki');
    expect(rows[2]).toHaveTextContent('/followup');
  });

  it('says what each command does and where that switch stands', async () => {
    const { textarea } = renderComposer();
    await userEvent.type(textarea, '/deep');

    const row = screen.getByRole('option');
    expect(row).toHaveTextContent('Multi-step research across sources. Slower.');
    expect(row).toHaveTextContent('Off');
  });

  it('moves the lit row with the arrow keys', async () => {
    const { textarea } = renderComposer();
    await userEvent.type(textarea, '/w');
    expect(screen.getAllByRole('option')).toHaveLength(3);
    expect(screen.getAllByRole('option')[0]).toHaveAttribute('aria-selected', 'true');

    await userEvent.keyboard('{ArrowDown}{ArrowDown}');
    expect(screen.getAllByRole('option')[2]).toHaveAttribute('aria-selected', 'true');

    // Down from the last row comes back to the first.
    await userEvent.keyboard('{ArrowDown}');
    expect(screen.getAllByRole('option')[0]).toHaveAttribute('aria-selected', 'true');

    await userEvent.keyboard('{ArrowUp}');
    expect(screen.getAllByRole('option')[2]).toHaveAttribute('aria-selected', 'true');
  });

  it('accepts on Enter, takes the token back out, and does not send', async () => {
    const { textarea, onSend, onRun } = renderComposer();
    await userEvent.type(textarea, 'find me /web');
    await userEvent.keyboard('{Enter}');

    expect(onRun).toHaveBeenCalledWith('web');
    expect(onSend).not.toHaveBeenCalled();
    expect(textarea).toHaveValue('find me ');
  });

  it('accepts on Tab as well', async () => {
    const { textarea, onRun } = renderComposer();
    await userEvent.type(textarea, '/wiki');
    await userEvent.tab();

    expect(onRun).toHaveBeenCalledWith('wiki');
  });

  it('accepts the row that is clicked', async () => {
    const { textarea, onRun } = renderComposer();
    await userEvent.type(textarea, '/w');
    await userEvent.click(screen.getByRole('option', { name: /\/wiki/ }));

    expect(onRun).toHaveBeenCalledWith('wiki');
  });

  it('closes on Escape and leaves the typed text alone', async () => {
    const { textarea, onRun } = renderComposer();
    await userEvent.type(textarea, '/web');
    await userEvent.keyboard('{Escape}');

    expect(screen.queryAllByRole('option')).toHaveLength(0);
    expect(onRun).not.toHaveBeenCalled();
    expect(textarea).toHaveValue('/web');
  });

  it('sends once the popup is shut', async () => {
    const { textarea, onSend } = renderComposer();
    await userEvent.type(textarea, 'how hot was July?');
    await userEvent.keyboard('{Enter}');

    expect(onSend).toHaveBeenCalledWith('how hot was July?');
  });

  it('never accepts or sends the Enter that commits an IME candidate', async () => {
    const { textarea, onSend, onRun } = renderComposer();
    await userEvent.type(textarea, '/web');

    // An IME commits its candidate with an Enter that reaches the composer as
    // an ordinary keydown. It is neither an accept nor a send.
    textarea.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', isComposing: true, bubbles: true })
    );

    expect(onRun).not.toHaveBeenCalled();
    expect(onSend).not.toHaveBeenCalled();
    expect(textarea).toHaveValue('/web');
  });

  it('leaves Shift+Enter to the composer, open list or not', async () => {
    const { textarea, onSend, onRun } = renderComposer();
    await userEvent.type(textarea, '/web');
    await userEvent.keyboard('{Shift>}{Enter}{/Shift}');

    expect(onRun).not.toHaveBeenCalled();
    expect(onSend).not.toHaveBeenCalled();
  });
});
