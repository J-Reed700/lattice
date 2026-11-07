import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { StarterList } from '@/features/chat/components/StarterList';

describe('StarterList', () => {
  it('lists the starters and hands the one picked to onPick', async () => {
    const onPick = vi.fn();
    render(<StarterList starters={['Quiz me', 'Teach me this']} onPick={onPick} />);

    expect(screen.getAllByRole('listitem')).toHaveLength(2);
    await userEvent.click(screen.getByRole('button', { name: 'Teach me this' }));
    expect(onPick).toHaveBeenCalledWith('Teach me this');
  });

  it('says its note above the starters', () => {
    const { container } = render(<StarterList starters={['Quiz me']} note="Ask about this passage." onPick={vi.fn()} />);

    expect(screen.getByText('Ask about this passage.')).toBeInTheDocument();
    expect(container).toHaveTextContent(/Ask about this passage\.\s*Quiz me/);
  });

  it('says its note alone, with no empty list, when there are no starters', () => {
    render(<StarterList starters={[]} note="Ask about this passage." onPick={vi.fn()} />);

    expect(screen.getByText('Ask about this passage.')).toBeInTheDocument();
    expect(screen.queryByRole('list')).not.toBeInTheDocument();
  });

  it('shows nothing when it has neither', () => {
    const { container } = render(<StarterList starters={[]} onPick={vi.fn()} />);

    expect(container).toBeEmptyDOMElement();
  });
});
