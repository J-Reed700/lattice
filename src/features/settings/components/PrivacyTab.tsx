import { PageHeader, SettingsRow, SettingsSection, Switch } from '@/components/ui';
import { toast } from '@/stores/toastStore';

import { SWITCH_CLASS } from './settingsStyles';
import { useSettingsQuery, useUpdateSettingsMutation } from '../hooks/useSettingsQuery';

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
