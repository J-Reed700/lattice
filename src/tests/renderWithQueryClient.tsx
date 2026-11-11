import type { ReactElement, ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import {
  render as baseRender,
  renderHook as baseRenderHook,
  type RenderHookOptions,
  type RenderOptions,
} from '@testing-library/react';

/** A wrapper around a fresh query client, so no cache outlives one render. */
export function createQueryWrapper() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function QueryWrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  };
}

/** Testing Library's `render` inside a `QueryClientProvider`. */
export function render(ui: ReactElement, options?: Omit<RenderOptions, 'wrapper'>) {
  return baseRender(ui, { ...options, wrapper: createQueryWrapper() });
}

/** Testing Library's `renderHook` inside a `QueryClientProvider`. */
export function renderHook<Result, Props>(
  callback: (initialProps: Props) => Result,
  options?: Omit<RenderHookOptions<Props>, 'wrapper'>,
) {
  return baseRenderHook(callback, { ...options, wrapper: createQueryWrapper() });
}
