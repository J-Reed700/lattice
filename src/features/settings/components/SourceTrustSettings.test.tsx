import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SourceTrustSettings } from './SourceTrustSettings';

type Query = { data?: unknown; isLoading?: boolean; error?: Error | null };
type Mutation = { mutate: ReturnType<typeof vi.fn>; isPending: boolean; error: Error | null; data?: unknown; isSuccess?: boolean };

const h = vi.hoisted(() => ({
  overrides: {} as Query,
  calibration: {} as Query,
  checker: {} as Query,
  clear: {} as Mutation,
  exported: {} as Mutation,
}));

vi.mock('@/features/learning/sources/trust/useSourceTrust', () => ({
  useSourceTrustOverrides: () => h.overrides,
  useSourceTrustCalibration: () => h.calibration,
  useCheckerReliability: () => h.checker,
  useClearSourceTrustOverride: () => h.clear,
  useExportProvenanceEval: () => h.exported,
}));

const siteOverride = {
  trustOverride: { scope: 'site', registrableDomain: 'britannica.com', standing: 'institutional', note: 'Edited encyclopedia', setAt: 1 },
  trackRecord: { right: 3, wrong: 1 },
};
const subjectOverride = {
  trustOverride: { scope: 'site_subject', registrableDomain: 'kew.org', class: { id: 'botany', label: 'Botany' }, level: 'primary', note: null, setAt: 2 },
  trackRecord: { right: 0, wrong: 0 },
};
const sourceOverride = {
  trustOverride: { scope: 'source', programId: 'p1', sourceId: 's1', level: 'established', note: null, setAt: 3 },
  trackRecord: { right: 0, wrong: 1 },
};

const bar = (level: string, overrides: Record<string, unknown> = {}) => ({
  level, acceptedAlone: false, basis: 'static', held: 0, failed: 0, since: null, ...overrides,
});

const calibrationData = {
  verdicts: 12,
  policyVersion: 3,
  kinds: [
    {
      claimKind: 'historical_date',
      lowestAccepted: 'authoritative',
      levels: [
        bar('primary', { acceptedAlone: true, held: 4, failed: 0 }),
        bar('authoritative', { acceptedAlone: true, held: 3, failed: 1, basis: 'relaxed', since: 1_700_000_000 }),
        bar('established', { basis: 'tightened', held: 1, failed: 0, since: null }),
      ],
    },
    {
      claimKind: 'statistic',
      lowestAccepted: null,
      levels: [bar('primary'), bar('authoritative'), bar('established', { basis: 'tightened', since: Number.NaN })],
    },
  ],
  changes: [{ claimKind: 'historical_date', level: 'authoritative', at: 5, claimId: 'c1', reason: { text: 'Authoritative relaxed after 4 held verdicts' } }],
  parameters: [{ name: 'min_verdicts', value: '4', derivation: 'Smallest sample with a 90% bound' }],
};

const idleMutation = (): Mutation => ({ mutate: vi.fn(), isPending: false, error: null, data: undefined, isSuccess: false });

describe('SourceTrustSettings', () => {
  beforeEach(() => {
    h.overrides = { data: [siteOverride, subjectOverride, sourceOverride], isLoading: false, error: null };
    h.calibration = { data: calibrationData, isLoading: false, error: null };
    h.checker = { data: [{ checkerModel: 'qwen-9b', labelled: 1, falseFlagRate: 0.1, falsePassRate: null, verifierLabelled: 0, retrievalMisses: 2 }], isLoading: false, error: null };
    h.clear = idleMutation();
    h.exported = idleMutation();
  });

  it('lists site overrides with their notes and track records', () => {
    render(<SourceTrustSettings />);
    expect(screen.getByText('britannica.com treated as institutional everywhere')).toBeInTheDocument();
    expect(screen.getByText('“Edited encyclopedia”')).toBeInTheDocument();
    expect(screen.getByText('1 of 4 claims its pages supported was overturned.')).toBeInTheDocument();
    expect(screen.getByText('Trust kew.org as primary for: Botany')).toBeInTheDocument();
    expect(screen.getAllByText('No verdicts on its pages yet.')).toHaveLength(1);
    expect(screen.getByText('1 of 1 claim its pages supported was overturned.')).toBeInTheDocument();
  });

  it('resets a site override by its scope, but never a per-source one', () => {
    render(<SourceTrustSettings />);
    fireEvent.click(screen.getByRole('button', { name: 'Reset: britannica.com treated as institutional everywhere' }));
    expect(h.clear.mutate).toHaveBeenCalledWith({ scope: 'site', registrableDomain: 'britannica.com' });
    fireEvent.click(screen.getByRole('button', { name: 'Reset: Trust kew.org as primary for: Botany' }));
    expect(h.clear.mutate).toHaveBeenCalledWith({ scope: 'site_subject', registrableDomain: 'kew.org', class: { id: 'botany', label: 'Botany' } });
    expect(screen.getAllByRole('button', { name: /^Reset:/ })).toHaveLength(2);
  });

  it('disables reset while one runs and shows a reset error', () => {
    h.clear = { ...idleMutation(), isPending: true, error: new Error('override in use') };
    render(<SourceTrustSettings />);
    for (const button of screen.getAllByRole('button', { name: /^Reset:/ })) expect(button).toBeDisabled();
    expect(screen.getByText('override in use')).toBeInTheDocument();
  });

  it('shows the bars in force for each claim kind', () => {
    render(<SourceTrustSettings />);
    expect(screen.getByText('12 verdicts recorded · policy 3')).toBeInTheDocument();
    expect(screen.getByRole('columnheader', { name: 'Primary' })).toBeInTheDocument();
    expect(screen.getByText('Alone from authoritative')).toBeInTheDocument();
    expect(screen.getByText('Only corroboration')).toBeInTheDocument();
    expect(screen.getByText('4 verdicts, 100% held')).toBeInTheDocument();
    expect(screen.getByText('4 verdicts, 75% held')).toBeInTheDocument();
    expect(screen.getByText('1 verdict, 100% held')).toBeInTheDocument();
    expect(screen.getByText(`Relaxed since ${new Date(1_700_000_000 * 1000).toISOString().slice(0, 10)}`)).toBeInTheDocument();
    expect(screen.getAllByText('Tightened')).toHaveLength(2);
    expect(screen.getAllByText('Not alone').length).toBeGreaterThan(0);
    expect(screen.getByText('Authoritative relaxed after 4 held verdicts')).toBeInTheDocument();
    expect(screen.getByText('min_verdicts = 4')).toBeInTheDocument();
    expect(screen.getByText('Smallest sample with a 90% bound')).toBeInTheDocument();
  });

  it('says when no bar has moved and counts one verdict in the singular', () => {
    h.calibration = { data: { ...calibrationData, verdicts: 1, changes: [] }, isLoading: false, error: null };
    render(<SourceTrustSettings />);
    expect(screen.getByText('1 verdict recorded · policy 3')).toBeInTheDocument();
    expect(screen.getByText('None: every bar is the design’s table.')).toBeInTheDocument();
  });

  it('shows loading states', () => {
    h.overrides = { isLoading: true };
    h.calibration = { isLoading: true };
    h.checker = { isLoading: true };
    render(<SourceTrustSettings />);
    expect(screen.getAllByRole('status')).toHaveLength(3);
    expect(screen.queryByText('Policy')).not.toBeInTheDocument();
  });

  it('shows load errors', () => {
    h.overrides = { error: new Error('overrides unreadable') };
    h.calibration = { error: new Error('calibration unreadable'), data: undefined };
    h.checker = { error: new Error('checker unreadable') };
    render(<SourceTrustSettings />);
    expect(screen.getAllByRole('alert').map((node) => node.textContent)).toEqual([
      'overrides unreadable', 'calibration unreadable', 'checker unreadable',
    ]);
  });

  it('explains empty overrides and an unlabelled checker', () => {
    h.overrides = { data: [] };
    h.calibration = { data: { ...calibrationData, kinds: [] } };
    h.checker = { data: [] };
    render(<SourceTrustSettings />);
    expect(screen.getByText('None yet. Set one from a source’s “Change trust…” menu.')).toBeInTheDocument();
    expect(screen.getByText('No labelled claims yet. Each confirmation, removal, edit or added source labels one.')).toBeInTheDocument();
  });

  it('reports checker accuracy per model', () => {
    render(<SourceTrustSettings />);
    expect(screen.getByText('qwen-9b:')).toBeInTheDocument();
    expect(screen.getByText(/1 labelled claim · false flags/)).toBeInTheDocument();
  });

  describe('provenance export', () => {
    it('exports and reports where it saved', () => {
      const first = render(<SourceTrustSettings />);
      fireEvent.click(screen.getByRole('button', { name: 'Export provenance eval' }));
      expect(h.exported.mutate).toHaveBeenCalled();
      first.unmount();
      h.exported = { ...idleMutation(), isSuccess: true, data: { examples: 1, path: '/evals/provenance/labels.jsonl' } };
      const second = render(<SourceTrustSettings />);
      expect(screen.getByText('Saved 1 example to /evals/provenance/labels.jsonl.')).toBeInTheDocument();
      second.unmount();
      h.exported = { ...idleMutation(), isSuccess: true, data: { examples: 7, path: '/e.jsonl' } };
      render(<SourceTrustSettings />);
      expect(screen.getByText('Saved 7 examples to /e.jsonl.')).toBeInTheDocument();
    });

    it('notes a cancelled export, progress and failure', () => {
      h.exported = { ...idleMutation(), isSuccess: true, data: null };
      const first = render(<SourceTrustSettings />);
      expect(screen.getByText('Export cancelled.')).toBeInTheDocument();
      first.unmount();
      h.exported = { ...idleMutation(), isPending: true, error: new Error('disk full') };
      render(<SourceTrustSettings />);
      expect(screen.getByRole('button', { name: 'Exporting…' })).toBeDisabled();
      expect(screen.getByText('disk full')).toBeInTheDocument();
    });
  });
});
