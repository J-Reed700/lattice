/**
 * Display settings.
 *
 * Theme is the only display preference the app actually acts on. It is read
 * and written through the backend `ui.theme` slice (React Query is the
 * mirror; the Rust SettingsRepository is the SSOT) because `useApplyTheme`
 * applies it from there.
 *
 * Font size, compact mode and file previews used to live here. Nothing in
 * the app ever read them — they were deleted in the September 2026 overhaul
 * rather than restyled. A control that changes nothing is a lie.
 */

import { useMemo } from 'react';

import { RefreshCw } from 'lucide-react';

import { cn } from '@/lib/utils';

import { SECONDARY_BUTTON_CLASS } from './settingsStyles';
import { useSettingsQuery, useUpdateSettingsMutation } from '../../hooks/queries/useSettingsQuery';
import {
  useCheckForUpdatesMutation,
  useVersionInfoQuery,
} from '../../hooks/queries/useUpdateCheckQuery';
import { useRegisterPaletteCommands } from '../../hooks/useRegisterPaletteCommands';
import { PageHeader, SettingsRow, SettingsSection } from '../ui';

import type { PaletteCommand } from '../../stores/paletteCommandsStore';

const THEMES = [
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
  { value: 'system', label: 'System' },
] as const;

export function DisplayTab() {
  const { data: settings, isLoading, isError, refetch } = useSettingsQuery();
  const updateSettings = useUpdateSettingsMutation();
  const activeTheme = settings?.ui.theme ?? 'system';

  const { data: versionInfo } = useVersionInfoQuery();
  const checkForUpdates = useCheckForUpdatesMutation();
  const checkMutate = checkForUpdates.mutate;
  const isChecking = checkForUpdates.isPending;
  const update = checkForUpdates.data;

  const paletteCommands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'app.checkUpdates',
        label: 'Check for updates',
        group: 'Vault',
        icon: RefreshCw,
        enabled: !isChecking,
        run: () => checkMutate(),
      },
    ],
    [isChecking, checkMutate],
  );
  useRegisterPaletteCommands(paletteCommands);

  const updateHint = isChecking
    ? 'Checking…'
    : checkForUpdates.isError
      ? "Couldn't check for updates. Try again in a minute."
      : update
        ? update.available && update.latestVersion
          ? `Version ${update.latestVersion} is available.`
          : 'Up to date.'
        : undefined;

  return (
    <>
      <PageHeader title="Display" />

      <SettingsSection title="Appearance">
        <SettingsRow label="Theme">
          <div
            role="radiogroup"
            aria-label="Theme"
            className="inline-flex items-center gap-0.5 rounded-sm border border-border-default p-0.5"
          >
            {THEMES.map((theme) => {
              const isActive = activeTheme === theme.value;
              return (
                <button
                  key={theme.value}
                  type="button"
                  role="radio"
                  aria-checked={isActive}
                  tabIndex={isActive ? 0 : -1}
                  onKeyDown={(event) => {
                    const direction = ['ArrowRight', 'ArrowDown'].includes(event.key) ? 1
                      : ['ArrowLeft', 'ArrowUp'].includes(event.key) ? -1 : 0;
                    if (!direction) return;
                    event.preventDefault();
                    const index = THEMES.findIndex((item) => item.value === theme.value);
                    const next = THEMES[(index + direction + THEMES.length) % THEMES.length];
                    const buttons = event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="radio"]');
                    buttons?.[(index + direction + THEMES.length) % THEMES.length]?.focus();
                    updateSettings.mutate({ category: 'ui', updates: { theme: next.value } });
                  }}
                  disabled={isLoading || !settings || updateSettings.isPending}
                  onClick={() =>
                    updateSettings.mutate({ category: 'ui', updates: { theme: theme.value } })
                  }
                  className={cn(
                    'rounded-sm px-3 py-1 text-sm transition-colors duration-fast disabled:cursor-not-allowed disabled:opacity-50',
                    isActive
                      ? 'bg-accent-muted text-accent'
                      : 'text-text-secondary hover:text-text-primary',
                  )}
                >
                  {theme.label}
                </button>
              );
            })}
          </div>
        </SettingsRow>
        {isError && (
          <p role="alert" className="text-sm text-danger">
            Couldn't load your appearance settings.{' '}
            <button type="button" className="underline" onClick={() => void refetch()}>Try again</button>
          </p>
        )}
        {updateSettings.isError && (
          <p role="alert" className="text-sm text-danger">Couldn't save your theme. Try selecting it again.</p>
        )}
      </SettingsSection>

      <SettingsSection title="About">
        <SettingsRow
          label="Version"
          hint={versionInfo?.commitHash ? `Build ${versionInfo.commitHash}` : undefined}
        >
          <span className="text-sm tabular-nums text-text-primary">
            {versionInfo?.version ?? '—'}
          </span>
        </SettingsRow>
        <SettingsRow label="Updates" hint={updateHint}>
          <div className="flex items-center gap-3">
            {update?.available && update.downloadUrl ? (
              <a
                href={update.downloadUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="text-sm text-accent hover:underline"
              >
                Download
              </a>
            ) : null}
            <button
              type="button"
              onClick={() => checkMutate()}
              disabled={isChecking}
              className={SECONDARY_BUTTON_CLASS}
            >
              {isChecking ? 'Checking…' : 'Check for updates'}
            </button>
          </div>
        </SettingsRow>
      </SettingsSection>
    </>
  );
}
