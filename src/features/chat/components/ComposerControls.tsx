import { BookOpen, Database, Globe, ShieldCheck, Telescope, Wrench } from 'lucide-react';

import { formatToolLabel, WEB_TOOL_NAMES, WIKI_TOOL_NAMES } from '@/features/chat/components/composer/toolNames';
import type { CustomToolSettings, ToolPreferences } from '@/types';

export type TurnMode = 'auto' | 'followup' | 'query';

const DEEP_RESEARCH_WARNING_MESSAGE =
  'Deep research runs multiple rounds of search. Expect a longer wait.';

interface ComposerControlsProps {
  turnMode: TurnMode;
  onTurnModeChange: (next: TurnMode) => void;
  toolPreferences: ToolPreferences;
  onToggleKnowledgeBase: () => void;
  onToggleStrictSources: () => void;
  onToggleWebTools: () => void;
  onToggleWikiTools: () => void;
  onToggleDeepResearch: () => void;
  customTools: CustomToolSettings[];
  enabledToolSet: Set<string>;
  onToggleCustomTool: (toolName: string) => void;
}

export function ComposerControls({
  turnMode,
  onTurnModeChange,
  toolPreferences,
  onToggleKnowledgeBase,
  onToggleStrictSources,
  onToggleWebTools,
  onToggleWikiTools,
  onToggleDeepResearch,
  customTools,
  enabledToolSet,
  onToggleCustomTool,
}: ComposerControlsProps) {
  const wikiEnabled = WIKI_TOOL_NAMES.some((toolName) => enabledToolSet.has(toolName));
  const deepResearchEnabled = Boolean(toolPreferences.deepResearchMode);

  return (
    <div className="space-y-4 text-sm">
      {/* Turn mode segmented control */}
      <section>
        <p className="mb-2 text-xs font-medium text-text-secondary">
          Turn mode
        </p>
        <div
          className="flex h-8 items-center gap-0.5 rounded-lg bg-[hsl(var(--text-primary)/0.06)] p-[3px]"
          role="tablist"
        >
          {([
            { id: 'auto' as const, label: 'Auto', description: 'Auto-detect new topic or follow-up' },
            { id: 'followup' as const, label: 'Follow-up', description: 'Continue the current topic' },
            { id: 'query' as const, label: 'Query', description: 'Always search sources before answering' },
          ]).map((opt) => {
            const active = turnMode === opt.id;
            return (
              <button
                key={opt.id}
                type="button"
                role="tab"
                aria-selected={active}
                onClick={() => onTurnModeChange(opt.id)}
                title={opt.description}
                className={`h-full flex-1 rounded-md px-2 text-xs font-medium transition-[background-color,color,box-shadow] duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring ${
                  active
                    ? 'bg-surface-overlay text-text-primary shadow-control'
                    : 'text-text-tertiary hover:text-text-primary'
                }`}
              >
                {opt.label}
              </button>
            );
          })}
        </div>
      </section>

      {/* Tools checklist */}
      <section>
        <p className="mb-2 text-xs font-medium text-text-secondary">
          Tools
        </p>
        <div className="space-y-1">
          <ToolRow
            icon={Database}
            label="Your documents"
            description="Search your library"
            checked={toolPreferences.knowledgeBase}
            onToggle={onToggleKnowledgeBase}
          />
          <ToolRow
            icon={ShieldCheck}
            label="Sources only"
            description="Answer strictly from retrieved sources; no general knowledge"
            checked={Boolean(toolPreferences.strictSources)}
            onToggle={onToggleStrictSources}
          />
          <ToolRow
            icon={Globe}
            label="Web"
            description="Search the web"
            checked={toolPreferences.webSearch}
            onToggle={onToggleWebTools}
          />
          <ToolRow
            icon={BookOpen}
            label="Wikipedia"
            description="Search and summarize Wikipedia"
            checked={wikiEnabled}
            onToggle={onToggleWikiTools}
          />
          <ToolRow
            icon={Telescope}
            label="Deep research"
            description="Multi-step research across sources. Slower."
            checked={deepResearchEnabled}
            onToggle={onToggleDeepResearch}
          />
          {customTools.map((tool) => (
            <ToolRow
              key={tool.name}
              icon={Wrench}
              label={formatToolLabel(tool.name)}
              description={tool.description || tool.name}
              checked={enabledToolSet.has(tool.name)}
              onToggle={() => onToggleCustomTool(tool.name)}
            />
          ))}
        </div>
        {deepResearchEnabled && (
          <p
            role="status"
            aria-live="polite"
            className="mt-2 rounded-sm border border-[hsl(var(--warning-muted))] bg-[hsl(var(--warning-muted))] px-2.5 py-1.5 text-xs text-[hsl(var(--warning-fg))]"
          >
            {DEEP_RESEARCH_WARNING_MESSAGE}
          </p>
        )}
      </section>
    </div>
  );
}

interface ToolRowProps {
  icon: typeof Database;
  label: string;
  description: string;
  checked: boolean;
  onToggle: () => void;
}

function ToolRow({ icon: Icon, label, description, checked, onToggle }: ToolRowProps) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      onClick={onToggle}
      className="flex w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left transition-colors duration-fast hover:bg-surface focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
    >
      <span
        className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border transition-colors duration-fast ${
          checked
            ? 'border-accent/30 bg-accent-muted text-accent'
            : 'border-border-subtle bg-surface text-text-tertiary'
        }`}
        aria-hidden="true"
      >
        <Icon className="h-3.5 w-3.5" strokeWidth={1.75} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-ui font-medium text-text-primary">{label}</span>
        <span className="block text-xs leading-snug text-text-muted">{description}</span>
      </span>
      <span
        aria-hidden="true"
        className={`relative inline-flex h-4 w-7 shrink-0 items-center rounded-full transition-colors duration-fast ${
          checked ? 'bg-accent' : 'bg-border-default'
        }`}
      >
        <span
          className={`block h-3 w-3 rounded-full bg-surface shadow-sm transition-transform duration-fast ${
            checked ? 'translate-x-3.5' : 'translate-x-0.5'
          }`}
        />
      </span>
    </button>
  );
}

export { DEEP_RESEARCH_WARNING_MESSAGE, WEB_TOOL_NAMES, WIKI_TOOL_NAMES };
