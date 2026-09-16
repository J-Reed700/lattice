import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { TypeFacets } from './TypeFacets';

const buckets = [
  { label: 'PDF', count: 980 },
  { label: 'Markdown', count: 210 },
  { label: 'Text', count: 57 },
];

describe('TypeFacets', () => {
  it('renders one button per bucket as "count label"', () => {
    render(<TypeFacets buckets={buckets} activeType={null} onSelect={vi.fn()} />);
    expect(screen.getByRole('tab', { name: '980 PDF' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '210 Markdown' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '57 Text' })).toBeInTheDocument();
  });

  it('selects an inactive facet', async () => {
    const onSelect = vi.fn();
    const user = userEvent.setup();
    render(<TypeFacets buckets={buckets} activeType={null} onSelect={onSelect} />);
    await user.click(screen.getByRole('tab', { name: '980 PDF' }));
    expect(onSelect).toHaveBeenCalledWith('PDF');
  });

  it('clears the active facet when it is clicked again', async () => {
    const onSelect = vi.fn();
    const user = userEvent.setup();
    render(<TypeFacets buckets={buckets} activeType="PDF" onSelect={onSelect} />);
    await user.click(screen.getByRole('tab', { name: '980 PDF' }));
    expect(onSelect).toHaveBeenCalledWith(null);
  });

  it('shows Clear only while a facet is active', async () => {
    const onSelect = vi.fn();
    const { rerender } = render(
      <TypeFacets buckets={buckets} activeType={null} onSelect={onSelect} />
    );
    expect(screen.queryByRole('button', { name: 'Clear' })).not.toBeInTheDocument();

    rerender(<TypeFacets buckets={buckets} activeType="Markdown" onSelect={onSelect} />);
    const clear = screen.getByRole('button', { name: 'Clear' });
    await userEvent.setup().click(clear);
    expect(onSelect).toHaveBeenCalledWith(null);
  });

  it('caps the line at six buckets', () => {
    const many = Array.from({ length: 9 }, (_, index) => ({
      label: `T${index}`,
      count: 9 - index,
    }));
    render(<TypeFacets buckets={many} activeType={null} onSelect={vi.fn()} />);
    expect(screen.getAllByRole('tab')).toHaveLength(6);
  });
});
