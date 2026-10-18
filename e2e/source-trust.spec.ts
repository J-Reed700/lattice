import { expect, test, type Page } from '@playwright/test';

import { mockCommands } from './fixtures/tauri';
import { installLearningStudioBackend } from './helpers/learningStudioBackend';
import { openStudioSection } from './helpers/learningStudioNavigation';

/**
 * Source trust (source provenance §19): the field-notes source is an
 * established page on example.org; two waiting claims in one lesson cite it.
 * Trust calls are answered here and logged on `window.__TRUST_CALLS__`.
 */
async function installSourceTrust(page: Page) {
  await installLearningStudioBackend(page);
  await mockCommands(page, () => {
    const calls: Array<{ command: string; request: unknown }> = [];
    (window as unknown as { __TRUST_CALLS__: typeof calls }).__TRUST_CALLS__ = calls;
    const trust = {
      sourceVersionId: 'source-version-field-notes-1',
      level: 'established' as const,
      subject: null,
      reasons: [
        { code: 'identified_publisher', text: 'Identified publisher: Example Field Office (JSON-LD publisher)' },
        { code: 'content_nature_cap', text: 'Field notes are reporting, not documentation' },
      ],
      overrideScope: null,
    };
    const entry = (withImpact: boolean) => ({
      sourceId: 'logical-source-field-notes',
      sourceVersionId: 'source-version-field-notes-1',
      trust,
      standing: 'identified' as const,
      standingReasons: [{ code: 'publisher_metadata', text: 'JSON-LD publisher: Example Field Office' }],
      provenance: {
        sourceVersionId: 'source-version-field-notes-1', originKey: 'web:example.org', site: 'example.org', publisher: 'Example Field Office',
        author: null, siteName: null, publishedAt: 1_760_000_000, modifiedAt: null, archivedFrom: null, notExtracted: null, relations: [],
      },
      trackRecord: { right: 4, wrong: 1 },
      overrides: [],
      impact: withImpact ? { source: { versions: 1, claims: 2, lessons: 1 }, site: { versions: 1, claims: 2, lessons: 1 } } : null,
    });
    const bar = (claimKind: string, lowest: 'primary' | 'authoritative' | 'established') => ({
      claimKind,
      staticLowest: lowest,
      lowestAccepted: lowest,
      levels: (['primary', 'authoritative', 'established', 'community', 'unknown'] as const).map((level, index) => ({
        level,
        acceptedAlone: index <= ['primary', 'authoritative', 'established'].indexOf(lowest),
        basis: 'static' as const,
        held: level === 'established' ? 39 : 0,
        failed: level === 'established' ? 2 : 0,
        probabilityReliable: 0.5,
        since: null,
      })),
      reasons: [],
    });
    return {
      get_learning_source_trust: (args: Record<string, unknown>) => {
        const request = args.request as { includeImpact: boolean };
        return { sources: [entry(request.includeImpact)], claim: null };
      },
      set_source_trust_override: (args: Record<string, unknown>) => {
        calls.push({ command: 'set_source_trust_override', request: args.request });
        return {
          change: { scope: { scope: 'site' as const, registrableDomain: 'example.org' }, sourceVersionIds: ['source-version-field-notes-1'] },
          impact: { versions: 1, claims: 2, lessons: 1 },
        };
      },
      list_source_trust_overrides: () => [{
        trustOverride: {
          scope: 'site_subject' as const, registrableDomain: 'example.org', class: { label: 'Field methods', key: 'field methods' },
          level: 'primary' as const, note: null, setAt: 1_760_000_000,
        },
        trackRecord: { right: 7, wrong: 2 },
      }],
      get_source_trust_calibration: () => ({
        policyVersion: 'source-trust-v1',
        parameters: [{ name: 'bar_move_probability', value: 0.9, derivation: 'How sure calibration must be before it moves a bar.' }],
        kinds: [bar('specification', 'authoritative'), bar('practice', 'established')],
        verdicts: 41,
      }),
      get_learning_checker_accuracy: () => [],
    };
  }, undefined);
}

test('A source shows its level with the reasons, and a site is trusted as primary only for a subject', async ({ page }) => {
  await installSourceTrust(page);
  await page.goto('/studio');
  await page.getByRole('button', { name: /Reasoning from field observations/ }).click();
  await openStudioSection(page, 'Sources');

  // The row carries the level as words, and the list filters by it.
  const list = page.getByLabel('Source list');
  await expect(list.getByText('Established')).toBeVisible();
  await page.getByRole('combobox', { name: 'Filter by trust level' }).selectOption('primary');
  await expect(list.getByText('No sources match this filter.')).toBeVisible();
  await page.getByRole('combobox', { name: 'Filter by trust level' }).selectOption('any');
  await list.getByRole('button', { name: /Public field notes/ }).click();

  // "Why?" lists the reasons verbatim.
  const section = page.getByRole('region', { name: 'Source trust' });
  await section.getByRole('button', { name: /Established · Why\?/ }).click();
  const reasons = page.getByRole('dialog');
  await expect(reasons.getByText('Identified publisher: Example Field Office (JSON-LD publisher)')).toBeVisible();
  await expect(reasons.getByText('Your verdicts: 4 of 5 claims it supported held, 1 overturned')).toBeVisible();
  await page.keyboard.press('Escape');

  // A site alone takes a standing; primary needs a subject.
  await section.getByRole('button', { name: /Change trust/ }).click();
  await section.getByRole('radio', { name: /example\.org, everywhere/ }).check();
  await expect(section.getByRole('combobox', { name: 'Level' })).toHaveCount(0);
  await section.getByRole('radio', { name: /example\.org, for a subject/ }).check();
  await section.getByRole('combobox', { name: 'Level' }).selectOption('primary');
  await section.getByRole('textbox', { name: 'Another subject' }).fill('Field methods');
  await expect(section.getByText('Trust example.org as primary for: Field methods')).toBeVisible();
  await expect(section.getByText('2 claims waiting for review in 1 lesson cite this site; they will be re-decided.')).toBeVisible();
  await section.getByRole('button', { name: 'Apply and re-decide 2 claims' }).click();
  await expect(section.getByText(/^Saved\. 2 claims/)).toBeVisible();
  const calls = await page.evaluate(() => (window as unknown as { __TRUST_CALLS__: unknown[] }).__TRUST_CALLS__);
  expect(calls).toEqual([{
    command: 'set_source_trust_override',
    request: { scope: 'site_subject', registrableDomain: 'example.org', classLabel: 'Field methods', level: 'primary', note: null },
  }]);

  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test('Settings shows the bars in force, the site overrides and their record', async ({ page }) => {
  await installSourceTrust(page);
  await page.goto('/settings?tab=source-trust');
  await expect(page.getByRole('heading', { name: 'Source trust' })).toBeVisible();
  await expect(page.getByText('Trust example.org as primary for: Field methods')).toBeVisible();
  await expect(page.getByText('2 of 9 claims its pages supported were overturned.')).toBeVisible();
  const table = page.getByRole('table');
  await expect(table.getByText('41 verdicts recorded · policy source-trust-v1')).toBeVisible();
  await expect(table.getByRole('rowheader', { name: /Specification/ })).toContainText('Alone from authoritative');
  await expect(table.getByText('41 verdicts, 95% held').first()).toBeVisible();
  await expect(page.getByText('bar_move_probability = 0.9')).toBeVisible();
});
