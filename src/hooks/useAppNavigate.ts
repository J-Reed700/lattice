import { useMemo } from 'react';

import { useNavigate, type NavigateOptions, type To } from 'react-router';

import { diagnostics } from '@/utils/diagnostics';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

export type AppNavigate = {
  (to: To, options?: NavigateOptions): void;
  (delta: number): void;
};

/** UI navigation reports rejected router transitions without leaking a promise. */
export function useAppNavigate(): AppNavigate {
  const navigate = useNavigate();
  return useMemo(() => handleAsyncEvent(
    (to: To | number, options?: NavigateOptions) => {
      if (typeof to === 'number') return navigate(to);
      return options === undefined ? navigate(to) : navigate(to, options);
    },
    (error) => diagnostics.capture(error, 'Navigation'),
  ), [navigate]);
}
