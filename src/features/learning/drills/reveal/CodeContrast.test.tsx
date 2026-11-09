import type { ComponentProps } from 'react';

import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { CodeContrast, type ContrastFocus } from '@/features/learning/drills/reveal/CodeContrast';
import { CLEAN_FILES, OPERATOR_AFTER, OPERATOR_BEFORE, REVIEWED_FILES } from '@/features/learning/drills/reveal/revealTestData';

const PATHS = ['src/limiter.py', 'src/config.py'];

function show(props: Partial<ComponentProps<typeof CodeContrast>> = {}) {
  const onPathChange = vi.fn();
  const view = render(
    <CodeContrast
      language="python"
      reviewed={REVIEWED_FILES}
      clean={CLEAN_FILES}
      paths={PATHS}
      path="src/limiter.py"
      onPathChange={onPathChange}
      focus={null}
      {...props}
    />,
  );
  return { onPathChange, ...view };
}

const pane = (name: string) => screen.getByRole('group', { name });
const highlighted = (element: HTMLElement) =>
  [...element.querySelectorAll('.cm-line.cm-explorer-highlight')].map((line) => line.textContent);

const focusOn = (patch: Partial<ContrastFocus> = {}): ContrastFocus => ({
  path: 'src/limiter.py',
  label: 'src/limiter.py · line 13',
  reviewed: { startLine: 13, endLine: 13 },
  clean: { startLine: 13, endLine: 13 },
  nonce: 1,
  ...patch,
});

describe('CodeContrast', () => {
  it('titles the step plainly and labels each version', () => {
    show();
    expect(
      screen.getByRole('heading', { level: 2, name: 'The code you reviewed · The code without the issues' }),
    ).toBeInTheDocument();
    expect(within(pane('The code you reviewed')).getByText('src/limiter.py')).toBeInTheDocument();
    expect(within(pane('The code without the issues')).getByText('src/limiter.py')).toBeInTheDocument();
  });

  it('asks for the file whose tab was chosen', async () => {
    const { onPathChange } = show();
    await userEvent.click(screen.getByRole('tab', { name: 'src/config.py' }));
    expect(onPathChange).toHaveBeenCalledWith('src/config.py');
  });

  it('marks the tab of the file shown', () => {
    show({ path: 'src/config.py' });
    expect(screen.getByRole('tab', { name: 'src/config.py' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: 'src/limiter.py' })).toHaveAttribute('aria-selected', 'false');
    expect(screen.getByRole('tabpanel')).toHaveAccessibleName('src/config.py');
  });

  it('has no tabs for a single file', () => {
    show({ paths: ['src/limiter.py'] });
    expect(screen.queryByRole('tablist')).toBeNull();
    expect(screen.queryByRole('tabpanel')).toBeNull();
    expect(pane('The code you reviewed')).toBeInTheDocument();
  });

  it('says so when one version has no such file', () => {
    const extra = { path: 'src/extra.py', text: 'x = 1\n' };
    show({
      clean: [...CLEAN_FILES, extra],
      paths: [...PATHS, extra.path],
      path: extra.path,
    });
    expect(within(pane('The code you reviewed')).getByText('This version has no src/extra.py.')).toBeInTheDocument();
    expect(pane('The code without the issues').querySelector('.cm-content')).toHaveTextContent('x = 1');
  });

  it('highlights the lines in both versions, and announces them', () => {
    show({ focus: focusOn() });
    expect(highlighted(pane('The code you reviewed'))).toEqual([OPERATOR_BEFORE]);
    expect(highlighted(pane('The code without the issues'))).toEqual([OPERATOR_AFTER]);
    expect(screen.getByRole('status')).toHaveTextContent('Showing src/limiter.py · line 13');
  });

  it('highlights only the code reviewed when the lines cannot be found in the clean code', () => {
    show({ focus: focusOn({ clean: null }) });
    expect(highlighted(pane('The code you reviewed'))).toEqual([OPERATOR_BEFORE]);
    expect(highlighted(pane('The code without the issues'))).toEqual([]);
  });

  it('highlights nothing, and announces nothing, for lines in another file', () => {
    show({ focus: focusOn({ path: 'src/config.py', label: 'src/config.py · line 8' }) });
    expect(highlighted(pane('The code you reviewed'))).toEqual([]);
    expect(highlighted(pane('The code without the issues'))).toEqual([]);
    expect(screen.getByRole('status')).toBeEmptyDOMElement();
  });
});
