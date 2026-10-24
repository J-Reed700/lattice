import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';

export interface FirstRunDismissal {
  dismissed: boolean | undefined;
  isPending: boolean;
  dismiss: () => void;
}

export function useFirstRunDismissal(): FirstRunDismissal {
  const { data, isPending } = useSettingsQuery();
  const { mutate } = useUpdateSettingsMutation();

  const dismiss = () => {
    mutate({ category: 'onboarding', updates: { firstRunDismissed: true } });
  };

  return {
    dismissed: data?.onboarding?.firstRunDismissed,
    isPending,
    dismiss,
  };
}
