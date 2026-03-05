/**
 * Prompts Tab - System prompts, prompt templates, verification, and agentic RAG.
 */

import { useEffect, useState } from 'react';

import { ChevronDown, ChevronRight, FileText, ShieldCheck, ShieldOff } from 'lucide-react';

import { useLlmSettings } from './useLlmSettings';
import { TEXTAREA_CLASS } from './shared';
import { useSettingsStore } from '../../../stores/settingsStore';
import { toast } from '../../../stores/toastStore';

import type { LLMSettings as ApiLLMSettings } from '../../../types/api/settings';

export function PromptsTab() {
  const { llmSettings, isLoading, saveLlmUpdates } = useLlmSettings();
  const aiSettings = useSettingsStore((state) => state.settings.ai);
  const updateAI = useSettingsStore((state) => state.updateAI);

  const [systemPromptDraft, setSystemPromptDraft] = useState('');
  const [greetingPromptDraft, setGreetingPromptDraft] = useState('');
  const [ragPromptDraft, setRagPromptDraft] = useState('');
  const [noContextPromptDraft, setNoContextPromptDraft] = useState('');
  const [toolFollowupPromptDraft, setToolFollowupPromptDraft] = useState('');
  const [isVerificationHelpExpanded, setIsVerificationHelpExpanded] = useState(false);

  useEffect(() => {
    if (!llmSettings) return;
    setSystemPromptDraft(llmSettings.prompts.systemPrompt || '');
    setGreetingPromptDraft(llmSettings.prompts.greetingPromptTemplate || '');
    setRagPromptDraft(llmSettings.prompts.ragPromptTemplate || '');
    setNoContextPromptDraft(llmSettings.prompts.noContextPromptTemplate || '');
    setToolFollowupPromptDraft(llmSettings.prompts.toolFollowupPromptTemplate || '');
  }, [llmSettings]);

  const verificationEnabled = llmSettings?.verification?.enabled ?? true;

  const handleVerificationToggle = async (enabled: boolean) => {
    if (!llmSettings) return;
    if ((llmSettings.verification?.enabled ?? true) === enabled) return;

    const ok = await saveLlmUpdates({
      verification: {
        ...(llmSettings.verification || { enabled: true }),
        enabled,
      },
    });

    if (ok) {
      toast.success(enabled ? 'Response verification enabled' : 'Response verification disabled');
    }
  };

  const savePromptField = async (
    key: keyof ApiLLMSettings['prompts'],
    value: string
  ) => {
    if (!llmSettings) return;
    const trimmed = value.trim();
    if (trimmed === llmSettings.prompts[key]) return;
    await saveLlmUpdates({
      prompts: {
        ...llmSettings.prompts,
        [key]: trimmed,
      },
    });
  };

  return (
    <div className="space-y-8">
      {/* Header */}
      <div className="flex items-center gap-3 pb-4 border-b border-[var(--border-color)]">
        <div className="p-2 bg-[var(--accent-light)] rounded-lg">
          <FileText className="w-5 h-5 text-[var(--accent-primary)]" />
        </div>
        <div>
          <h2 className="text-xl font-semibold text-[var(--text-primary)]">Prompts</h2>
          <p className="text-sm text-[var(--text-secondary)]">
            System prompts, templates, verification, and retrieval behavior
          </p>
        </div>
      </div>

      {isLoading || !llmSettings ? (
        <div className="text-xs text-[var(--text-tertiary)]">Loading prompt settings...</div>
      ) : (
        <>
          {/* Verification */}
          <section className="space-y-4">
            <div className="space-y-3 rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] p-4">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <div className="text-sm font-medium text-[var(--text-primary)]">
                    Response Verification
                  </div>
                  <p className="mt-0.5 text-xs text-[var(--text-secondary)]">
                    Show verified and unverified claims in assistant messages.
                  </p>
                </div>
                <span
                  className={`inline-flex items-center gap-1 rounded-md border px-2 py-1 text-[11px] ${
                    verificationEnabled
                      ? 'border-emerald-400/35 bg-emerald-500/10 text-emerald-200'
                      : 'border-white/20 bg-white/5 text-white/70'
                  }`}
                >
                  {verificationEnabled ? (
                    <ShieldCheck className="h-3.5 w-3.5" />
                  ) : (
                    <ShieldOff className="h-3.5 w-3.5" />
                  )}
                  {verificationEnabled ? 'On' : 'Off'}
                </span>
              </div>

              <div className="grid grid-cols-2 gap-2">
                <button
                  type="button"
                  onClick={() => void handleVerificationToggle(true)}
                  className={`rounded-md border px-3 py-1.5 text-xs font-medium transition-colors ${
                    verificationEnabled
                      ? 'border-[var(--accent-primary)] bg-[var(--accent-light)] text-[var(--text-primary)]'
                      : 'border-[var(--border-color)] bg-[var(--bg-secondary)] text-[var(--text-secondary)] hover:border-[var(--border-hover)]'
                  }`}
                >
                  Turn On
                </button>
                <button
                  type="button"
                  onClick={() => void handleVerificationToggle(false)}
                  className={`rounded-md border px-3 py-1.5 text-xs font-medium transition-colors ${
                    !verificationEnabled
                      ? 'border-[var(--accent-primary)] bg-[var(--accent-light)] text-[var(--text-primary)]'
                      : 'border-[var(--border-color)] bg-[var(--bg-secondary)] text-[var(--text-secondary)] hover:border-[var(--border-hover)]'
                  }`}
                >
                  Turn Off
                </button>
              </div>

              <button
                type="button"
                onClick={() => setIsVerificationHelpExpanded((previous) => !previous)}
                className="inline-flex items-center gap-1 text-xs text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]"
              >
                {isVerificationHelpExpanded ? (
                  <ChevronDown className="w-3.5 h-3.5" />
                ) : (
                  <ChevronRight className="w-3.5 h-3.5" />
                )}
                {isVerificationHelpExpanded ? 'Hide details' : 'What changes when this is off?'}
              </button>

              {isVerificationHelpExpanded && (
                <ul className="space-y-1 text-xs text-[var(--text-tertiary)]">
                  <li>Chat messages won&apos;t include verification summaries.</li>
                  <li>Badges and verified/unverified claim details are hidden.</li>
                  <li>Turn it back on any time from this panel.</li>
                </ul>
              )}
            </div>
          </section>

          {/* Agentic RAG */}
          <section className="space-y-4">
            <div className="p-4 bg-[var(--bg-secondary)] rounded-lg">
              <div className="flex items-start gap-3">
                <input
                  id="enableAgenticRAG"
                  type="checkbox"
                  checked={aiSettings.enableAgenticRAG}
                  onChange={(e) => updateAI({ enableAgenticRAG: e.target.checked })}
                  className="mt-0.5 w-4 h-4 text-[var(--accent-primary)] bg-[var(--bg-tertiary)] border-[var(--border-color)] rounded focus:ring-2 focus:ring-[var(--accent-primary)]"
                />
                <label htmlFor="enableAgenticRAG" className="flex-1 cursor-pointer">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium text-[var(--text-primary)]">
                      Agentic RAG
                    </span>
                    <span className="text-xs bg-[var(--warning-light)] text-[var(--warning)] px-1.5 py-0.5 rounded">
                      Experimental
                    </span>
                  </div>
                  <p className="text-xs text-[var(--text-secondary)] mt-0.5">
                    AI agents autonomously refine searches and gather information. May increase
                    processing time.
                  </p>
                </label>
              </div>
            </div>
          </section>

          {/* Prompt Templates */}
          <section className="space-y-4">
            <div className="space-y-3 rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] p-4">
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-secondary)]">
                  System Prompt
                </label>
                <textarea
                  value={systemPromptDraft}
                  onChange={(e) => setSystemPromptDraft(e.target.value)}
                  onBlur={() => void savePromptField('systemPrompt', systemPromptDraft)}
                  rows={3}
                  className={TEXTAREA_CLASS}
                />
              </div>

              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-secondary)]">
                  Greeting Prompt Template
                </label>
                <textarea
                  value={greetingPromptDraft}
                  onChange={(e) => setGreetingPromptDraft(e.target.value)}
                  onBlur={() => void savePromptField('greetingPromptTemplate', greetingPromptDraft)}
                  rows={3}
                  className={TEXTAREA_CLASS}
                />
              </div>

              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-secondary)]">
                  RAG Prompt Template
                </label>
                <textarea
                  value={ragPromptDraft}
                  onChange={(e) => setRagPromptDraft(e.target.value)}
                  onBlur={() => void savePromptField('ragPromptTemplate', ragPromptDraft)}
                  rows={5}
                  className={TEXTAREA_CLASS}
                />
              </div>

              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-secondary)]">
                  No-Context Prompt Template
                </label>
                <textarea
                  value={noContextPromptDraft}
                  onChange={(e) => setNoContextPromptDraft(e.target.value)}
                  onBlur={() =>
                    void savePromptField('noContextPromptTemplate', noContextPromptDraft)
                  }
                  rows={4}
                  className={TEXTAREA_CLASS}
                />
              </div>

              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-secondary)]">
                  Tool Follow-up Prompt Template
                </label>
                <textarea
                  value={toolFollowupPromptDraft}
                  onChange={(e) => setToolFollowupPromptDraft(e.target.value)}
                  onBlur={() =>
                    void savePromptField('toolFollowupPromptTemplate', toolFollowupPromptDraft)
                  }
                  rows={4}
                  className={TEXTAREA_CLASS}
                />
              </div>
            </div>
          </section>
        </>
      )}
    </div>
  );
}
