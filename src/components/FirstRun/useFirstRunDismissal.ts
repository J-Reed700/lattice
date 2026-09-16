/**
 * First-run dismissal state.
 *
 * "Has the user already been offered the model bundle?" is state the app's
 * startup path acts on, so it belongs to the settings repository. Reads go
 * through `useSettingsQuery` and writes through `useUpdateSettingsMutation`.
 * The backend `onboarding.firstRunDismissed` setting is the only source of
 * truth — nothing about the first run is kept in `localStorage`.
 */

import { useSettingsQuery, useUpdateSettingsMutation } from '../../hooks/queries/useSettingsQuery';

export interface FirstRunDismissal {
  /** `undefined` until the settings document has been read. */
  dismissed: boolean | undefined;
  /** True while the first read is in flight. */
  isPending: boolean;
  /** Records the dismissal against the repository. */
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
