/**
 * activeRemoteRow
 *
 * Decides which remote-connection row the Downloaded tab previews: the
 * llama.cpp connection or the Ollama endpoint.
 *
 * The tab used to render the Ollama row unconditionally, so a user who set up
 * a llama.cpp connection still saw "Ollama connection · http://localhost:11434"
 * — the row for the connection they did NOT configure. This picks the row that
 * actually has something set up, mirroring the backend's own precedence: a
 * configured llama.cpp connection is tried before Ollama (see
 * `ModelLoader::load` / `select_model`), and an explicit provider always wins.
 *
 * Ollama's URL defaults to `http://localhost:11434`, so it is never "unset" —
 * the discriminator is whether the user bothered to configure a llama.cpp
 * connection (its URL defaults to empty).
 */

import type { LLMSettings } from '../../../types/api/settings';

export type ActiveRemoteRow = 'llamacpp' | 'ollama';

export function activeRemoteRow(settings: LLMSettings | null | undefined): ActiveRemoteRow {
  const provider = settings?.provider ?? 'auto';
  const llamaCppConfigured = (settings?.llamaCpp?.url ?? '').trim() !== '';

  // An explicit provider choice always wins, even if the other connection is
  // also saved. Picking llama.cpp with an empty URL still shows the llama.cpp
  // row (with its "Set a server URL" prompt) rather than a misleading Ollama.
  if (provider === 'ollama') {
    return 'ollama';
  }
  if (provider === 'llamacpp') {
    return 'llamacpp';
  }

  // Otherwise (Auto / local / other providers) a configured llama.cpp
  // connection takes priority over Ollama, matching the backend's Auto
  // precedence (llama.cpp is tried before Ollama).
  if (llamaCppConfigured) {
    return 'llamacpp';
  }

  return 'ollama';
}
