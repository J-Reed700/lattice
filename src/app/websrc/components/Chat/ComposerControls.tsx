import { BookOpen, Database, Globe, Sparkles, Wrench } from 'lucide-react';

import type { CustomToolSettings, ToolPreferences } from '../../types';

export type TurnMode = 'auto' | 'followup' | 'query';

const WEB_TOOL_NAMES = ['web_search', 'fetch_url_content'] as const;
const WIKI_TOOL_NAMES = ['wiki_search', 'wiki_summary'] as const;

const DEEP_RESEARCH_WARNING_MESSAGE =
  'Deep research runs multiple rounds of search. Expect a longer wait.';

interface ComposerControlsProps {
  turnMode: TurnMode;
  onTurnModeChange: (next: TurnMode) => void;
  toolPreferences: ToolPreferences;
  onToggleKnowledgeBase: () => void;
  onToggleWebTools: () => void;
  onToggleWikiTools: () => void;
  onToggleDeepResearch: () => void;
  customTools: CustomToolSettings[];
  enabledToolSet: Set<string>;
  onToggleCustomTool: (toolName: string) => void;
}

const formatToolLabel = (name: string): string =>
  name
    .split('_')
    .filter(Boolean)
    .map((segment) => segment.charAt(0).toUpperCase() + segment.slice(1))
    .join(' ');

export function ComposerControls({
  turnMode,
  onTurnModeChange,
  toolPreferences,
  onToggleKnowledgeBase,
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
        <p className="mb-2 text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
          Turn mode
        </p>
        <div
          className="flex items-center gap-1 rounded-sm border border-subtle bg-surface p-0.5"
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
                className={`flex-1 rounded-sm px-2 py-1 text-xs transition-colors duration-fast ${
                  active
                    ? 'bg-surface-raised text-[hsl(var(--text-primary))]'
                    : 'text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-secondary))]'
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
        <p className="mb-2 text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
          Tools
        </p>
        <div className="space-y-0.5">
          <ToolRow
            icon={Database}
            label="Knowledge base"
            description="Search indexed documents"
            checked={toolPreferences.knowledgeBase}
            onToggle={onToggleKnowledgeBase}
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
            icon={Sparkles}
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
      className="flex w-full items-start gap-2.5 rounded-sm px-2 py-1.5 text-left transition-colors duration-fast hover:bg-surface"
    >
      <span className="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-sm border border-default">
        {checked && (
          <span className="h-2 w-2 rounded-sm bg-[hsl(var(--accent))]" aria-hidden="true" />
        )}
      </span>
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-1.5 text-sm text-[hsl(var(--text-primary))]">
          <Icon className="h-3 w-3 text-[hsl(var(--text-tertiary))]" />
          {label}
        </span>
        <span className="block text-xs text-[hsl(var(--text-muted))]">{description}</span>
      </span>
    </button>
  );
}

export { DEEP_RESEARCH_WARNING_MESSAGE, WEB_TOOL_NAMES, WIKI_TOOL_NAMES };
