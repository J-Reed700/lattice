/**
 * Privacy settings.
 *
 * Reads from + writes to the backend Privacy settings slice via React Query.
 * The Rust SettingsRepository is the SSOT — Zustand is intentionally not
 * involved because backend-backed state uses React Query.
 *
 * The marketing panels that used to live here ("Privacy First", the
 * telemetry bullet lists, "Local Data Storage", "Current Status", and three
 * link buttons with no handlers) were removed in the September 2026
 * overhaul: none of them were controls, and the link buttons went nowhere.
 */

import { SWITCH_CLASS } from './settingsStyles';
import { useSettingsQuery, useUpdateSettingsMutation } from '../../hooks/queries/useSettingsQuery';
import { toast } from '../../stores/toastStore';
import { PageHeader, SettingsRow, SettingsSection, Switch } from '../ui';

const DEFAULT_PRIVACY = {
  telemetryEnabled: false,
  crashReporting: false,
};

export function PrivacyTab() {
  const { data: settings, isLoading } = useSettingsQuery();
  const updateMutation = useUpdateSettingsMutation();
  const privacySettings = settings?.privacy ?? DEFAULT_PRIVACY;
  const isSaving = updateMutation.isPending;

  const updatePrivacy = (updates: Partial<typeof DEFAULT_PRIVACY>) => {
    updateMutation.mutate(
      { category: 'privacy', updates },
      {
        onError: (error) => {
          toast.error("Couldn't save privacy setting", { message: error.message });
        },
      }
    );
  };

  return (
    <>
      <PageHeader title="Privacy" meta="Your documents and models stay on this device." />

      <SettingsSection title="What leaves this device">
        <SettingsRow
          label="Anonymous usage statistics"
          hint="Feature counts, performance timings, OS version. Never file names, content, or queries."
        >
          <Switch
            className={SWITCH_CLASS}
            checked={privacySettings.telemetryEnabled}
            disabled={isLoading || isSaving}
            onCheckedChange={(checked) => updatePrivacy({ telemetryEnabled: checked })}
            aria-label="Anonymous usage statistics"
          />
        </SettingsRow>

        <SettingsRow
          label="Crash reports"
          hint="Stack traces and system information, sanitized of file content."
        >
          <Switch
            className={SWITCH_CLASS}
            checked={privacySettings.crashReporting}
            disabled={isLoading || isSaving}
            onCheckedChange={(checked) => updatePrivacy({ crashReporting: checked })}
            aria-label="Crash reports"
          />
        </SettingsRow>
      </SettingsSection>
    </>
  );
}
