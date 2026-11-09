import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { MisconceptionCard } from './MisconceptionCard';

const card = vi.hoisted(() => vi.fn());
vi.mock('@/features/learning/practice/shared/SaveAsReviewCard', () => ({
  SaveAsReviewCard: (props: Record<string, unknown>) => {
    card(props);
    return <div data-testid="review-card" />;
  },
}));

describe('MisconceptionCard', () => {
  it('saves through the shared review card, asking what is wrong and what is right instead', () => {
    render(<MisconceptionCard programId="program-1" idea="An observation already proves the inference." />);
    expect(screen.getByTestId('review-card')).toBeInTheDocument();
    expect(card).toHaveBeenCalledWith(expect.objectContaining({
      programId: 'program-1',
      front: 'What is wrong with this idea, and what is right instead? “An observation already proves the inference.”',
      back: '',
    }));
  });
});
