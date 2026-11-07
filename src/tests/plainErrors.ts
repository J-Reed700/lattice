import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect } from 'vitest';

import type { ApiError } from '@/lib/bindings';

/**
 * A failed call as the IPC layer hands it to the app: `error` is the backend's
 * short `ApiError.message`, and the structured error (code and the real
 * cause in `details`) rides beside it. See `shared/ipc/transport.ts` and the
 * Rust mapping in `shared/ipc/conversions.rs`.
 */
export type FailedCall = { ok: false; error: string; details?: ApiError };

export function apiFailure(code: ApiError['code'], message: string, details: string | null): FailedCall {
  return { ok: false, error: message, details: { code, message, details } };
}

/** What `AppError::PromptExceedsWindow` says, as `rejection.rs` words it; its code is PROMPT_EXCEEDS_WINDOW and it has no details. */
export const TOO_LARGE_TEXT = 'llama.cpp rejected this request: 400 exceed_context_size_error. The request was 9,120 tokens; the configured context window is 8,192. The server refused it as too large. The prompt\'s size was estimated, not counted, so the request may not fit the configured context window of 8192 after all: shorten the request.';

export const NO_LEARNING_MODEL_TEXT = 'No Learning model is set. Choose one in Settings → Models under Learning.';

export type PlainErrorCase = {
  name: string;
  failure: FailedCall;
  /** The one plain sentence the learner reads first. */
  headline: string;
  /** The backend's own text, which belongs under Details. */
  raw: string;
};

/** The failures a learner meets while a course is built or a lesson is prepared, each as the backend really sends it. */
export const PLAIN_ERROR_CASES: PlainErrorCase[] = [
  {
    name: 'the model server cannot be reached',
    failure: apiFailure('NETWORK_ERROR', 'Network error', 'llama.cpp request failed: connection refused (os error 10061)'),
    headline: 'Lattice couldn\'t reach the model server.',
    raw: 'llama.cpp request failed: connection refused (os error 10061)',
  },
  {
    name: 'no Learning model is set',
    failure: apiFailure('LEARNING_MODEL_NOT_SET', 'No Learning model is set', NO_LEARNING_MODEL_TEXT),
    headline: 'Courses need a model to write them.',
    raw: NO_LEARNING_MODEL_TEXT,
  },
  {
    name: 'the model is rate limited',
    failure: apiFailure('RATE_LIMIT_EXCEEDED', 'Rate limit exceeded', 'llama.cpp returned 429 (rate_limit_error)'),
    headline: 'The model is getting too many requests. Wait a minute and try again.',
    raw: 'llama.cpp returned 429 (rate_limit_error)',
  },
  {
    name: 'the request is too large for the model',
    failure: apiFailure('PROMPT_EXCEEDS_WINDOW', TOO_LARGE_TEXT, null),
    headline: 'That is too much for the model to read at once. Try a smaller course or a model with a larger context window.',
    raw: TOO_LARGE_TEXT,
  },
  {
    name: 'the database fails',
    failure: apiFailure('DATABASE_ERROR', 'Database error', 'disk I/O error'),
    headline: 'Lattice couldn\'t save your work. Try again.',
    raw: 'disk I/O error',
  },
  {
    name: 'the bundled model server cannot start',
    failure: apiFailure('MODEL_LOAD_ERROR', 'AI model not available', 'llama-server exited with code 1 before it was ready'),
    headline: "The model server couldn't start. Reinstall Lattice, or choose another model in Settings.",
    raw: 'llama-server exited with code 1 before it was ready',
  },
  {
    name: 'the error is unknown (no code at all)',
    failure: { ok: false, error: 'socket hang up after 3 tries' },
    headline: 'Something went wrong.',
    raw: 'socket hang up after 3 tries',
  },
];

/** The alert that carries `headline`, found by its sentence rather than by role alone, since a page may hold several alerts. */
export async function findAlertWith(headline: string): Promise<HTMLElement> {
  const sentence = await screen.findByText(headline);
  const alert = sentence.closest<HTMLElement>('[role="alert"]');
  expect(alert, `"${headline}" is not inside a role="alert" element`).not.toBeNull();
  return alert!;
}

/**
 * The alert says the plain sentence first; the backend's text is not on show
 * until Details is opened, and then it is.
 */
export async function expectPlainError(alert: HTMLElement, { headline, raw }: Pick<PlainErrorCase, 'headline' | 'raw'>) {
  expect(within(alert).getByText(headline)).toBeVisible();
  for (const node of within(alert).queryAllByText(raw, { exact: false })) expect(node).not.toBeVisible();
  await userEvent.click(within(alert).getByText('Details'));
  const shown = within(alert).getAllByText(raw, { exact: false });
  expect(shown[0]).toBeVisible();
}
