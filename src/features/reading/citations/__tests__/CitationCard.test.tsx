import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type { RelationDto } from '@/lib/bindings';

import { CitationCard } from '../CitationCard';

const LABELS: [RelationDto, string][] = [
  ['quotes', 'Quoted from'],
  ['restates', 'Paraphrased from'],
  ['summarizes', 'Summarized from'],
  ['infers', 'Inferred from'],
  ['agrees', 'Consistent with'],
  ['backs', 'Supported by'],
  ['cites', 'Cited (not checked)'],
  ['contradicts', 'Contradicted by'],
];

describe('CitationCard', () => {
  it.each(LABELS)('says %s as "%s"', (relation, label) => {
    render(<CitationCard number={2} title="The Quillet Guide" relation={relation} excerpts={['Tea is hot.']} />);

    expect(screen.getByText(label)).toBeInTheDocument();
    expect(screen.getByRole('group', { name: /The Quillet Guide/ })).toBeInTheDocument();
  });

  it('has no relation line without a relation', () => {
    const view = render(<CitationCard number={2} title="The Quillet Guide" excerpts={['Tea is hot.']} />);

    for (const [, label] of LABELS) expect(view.queryByText(label)).toBeNull();
    expect(screen.getByRole('group', { name: 'Source 2, The Quillet Guide' })).toBeInTheDocument();
  });

  it('names the card from its relation', () => {
    render(<CitationCard number={2} title="The Quillet Guide" relation="restates" excerpts={[]} />);

    expect(screen.getByRole('group', { name: 'Source 2, paraphrased from The Quillet Guide' })).toBeInTheDocument();
  });

  it('puts the site after the title', () => {
    const view = render(<CitationCard number={2} title="The Quillet Guide" site="quillet.example" excerpts={[]} />);

    expect(view.container).toHaveTextContent('The Quillet Guide · quillet.example');
  });

  it('sets a quotation between curly quotes', () => {
    render(<CitationCard number={1} title="The Quillet Guide" relation="quotes" excerpts={['Tea is hot.']} />);

    expect(screen.getByText('“Tea is hot.”')).toBeInTheDocument();
  });

  it('sets a paraphrase without quote marks', () => {
    render(<CitationCard number={1} title="The Quillet Guide" relation="restates" excerpts={['Tea is hot.']} />);

    expect(screen.getByText('Tea is hot.')).toBeInTheDocument();
    expect(screen.queryByText('“Tea is hot.”')).toBeNull();
  });

  it('separates excerpts with an ellipsis', () => {
    const view = render(
      <CitationCard number={1} title="The Quillet Guide" excerpts={['First sentence.', 'Second sentence.']} />,
    );

    expect(view.container).toHaveTextContent('First sentence.…Second sentence.');
  });

  it('shows the passage when there are no excerpts', () => {
    render(<CitationCard number={1} title="The Quillet Guide" excerpts={[]} passage="A whole saved passage." />);

    expect(screen.getByText('A whole saved passage.')).toBeInTheDocument();
  });

  it('shows the heading trail and the footer', () => {
    render(
      <CitationCard
        number={1}
        title="The Quillet Guide"
        excerpts={['Tea is hot.']}
        trail={['Setup', 'Tea']}
        footer={<p>Click to open the source</p>}
      />,
    );

    expect(screen.getByText('Setup › Tea')).toBeInTheDocument();
    expect(screen.getByText('Click to open the source')).toBeInTheDocument();
  });

  it('offers Open source only when it can be opened', async () => {
    const onOpen = vi.fn();
    const view = render(<CitationCard number={1} title="The Quillet Guide" excerpts={[]} />);
    expect(view.queryByRole('button', { name: 'Open source' })).toBeNull();

    view.rerender(<CitationCard number={1} title="The Quillet Guide" excerpts={[]} onOpen={onOpen} />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Open source' }));
    expect(onOpen).toHaveBeenCalledTimes(1);
  });
});
