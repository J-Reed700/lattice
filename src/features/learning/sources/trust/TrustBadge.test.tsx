import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { TrustBadge, TrustLevelChip } from '@/features/learning/sources/trust/TrustBadge';
import { TRUST_LEVELS } from '@/features/learning/sources/trust/trustPresentation';
import { entryOf, trustOf } from '@/features/learning/sources/trust/trustTestData';

describe('Trust badge', () => {
  it('renders every level as words and a shape, never colour alone', () => {
    for (const level of TRUST_LEVELS) {
      const { container, unmount } = render(<TrustLevelChip trust={trustOf(level)} />);
      const chip = container.querySelector(`[data-trust-level="${level}"]`);
      expect(chip).not.toBeNull();
      expect(chip?.querySelector('svg')).not.toBeNull();
      expect(chip?.textContent?.trim().length).toBeGreaterThan(0);
      unmount();
    }
  });

  it('opens its reasons from the keyboard and lists them verbatim with the facts beside them', async () => {
    const trust = trustOf('primary', {
      subject: 'Python',
      reasons: [
        { code: 'owner_of_subject', text: 'python.org is the publisher of the Python documentation (JSON-LD publisher: Python Software Foundation)' },
        { code: 'institutional_editorial', text: 'Institutional publisher with an editorial process' },
      ],
    });
    const entry = entryOf(trust, {
      trackRecord: { right: 7, wrong: 2 },
      overrides: [{ scope: 'site_subject', registrableDomain: 'python.org', class: { label: 'Python', key: 'python' }, level: 'primary', note: null, setAt: 1 }],
    });
    render(<TrustBadge trust={trust} entry={entry} />);
    const badge = screen.getByRole('button', { name: /Primary for Python/ });
    badge.focus();
    await userEvent.keyboard('{Enter}');
    const dialog = await screen.findByRole('dialog');
    const reasons = within(within(dialog).getByRole('region', { name: 'Why this level' })).getAllByRole('listitem');
    expect(reasons.map((item) => item.textContent)).toEqual(trust.reasons.map((reason) => reason.text));
    expect(dialog).toHaveTextContent('Publisher standing: Identified');
    expect(dialog).toHaveTextContent('Author: Jane Doe');
    expect(dialog).toHaveTextContent('Your verdicts: 7 of 9 claims it supported held, 2 overturned');
    expect(dialog).toHaveTextContent('Trust python.org as primary for: Python');
  });
});
