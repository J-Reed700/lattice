import { useEffect, useId, useState } from 'react';

import { ChevronRight } from 'lucide-react';

import type { ModelRef } from '@/lib/bindings';
import { cn } from '@/lib/utils';
import { modelRefKey } from '@/utils/chatModelSelection';
import {
  CONSERVATIVE_DEFAULT,
  VISION_PRESETS,
  cloudVisionDefault,
  geometryProblem,
  maxPictureTokens,
  planPicture,
  presetGeometry,
  presetOf,
  sameGeometry,
  type VisionDeclaration,
  type VisionGeometry,
  type VisionPreset,
} from '@/utils/visionGeometry';

import { INPUT_CLASS, NUMBER_FIELD_CLASS } from './shared';
import { useLlmSettings } from './useLlmSettings';

type Choice = 'off' | 'conservative' | 'custom' | VisionPreset;

type GeometryKind = VisionGeometry['kind'];

/** The picture the cost preview prices: an ordinary 4:3 photo. */
const PREVIEW_SIZE = { width: 1280, height: 960 } as const;

const formatCount = (value: number) => value.toLocaleString('en-US');

const CONSERVATIVE_LABEL = `Conservative default (up to ${formatCount(maxPictureTokens(CONSERVATIVE_DEFAULT))} tokens per picture)`;

const KINDS: ReadonlyArray<{ kind: GeometryKind; label: string; template: VisionGeometry }> = [
  { kind: 'grid', label: 'Grid', template: CONSERVATIVE_DEFAULT },
  { kind: 'fixed', label: 'Fixed', template: presetGeometry('gemma_siglip') },
  { kind: 'tiles', label: 'Tiles', template: presetGeometry('open_ai') },
];

const FIELDS: { [K in GeometryKind]: ReadonlyArray<{ key: string; label: string }> } = {
  grid: [
    { key: 'token_side', label: 'Token side (px)' },
    { key: 'min_pixels', label: 'Min pixels' },
    { key: 'max_pixels', label: 'Max pixels' },
    { key: 'overhead_tokens', label: 'Overhead tokens' },
  ],
  fixed: [
    { key: 'side', label: 'Side (px)' },
    { key: 'tokens', label: 'Tokens' },
  ],
  tiles: [
    { key: 'tile_side', label: 'Tile side (px)' },
    { key: 'tokens_per_tile', label: 'Tokens per tile' },
    { key: 'base_tokens', label: 'Base tokens' },
    { key: 'max_tiles', label: 'Max tiles' },
    { key: 'max_side', label: 'Max side (px)' },
  ],
};

export interface VisionDeclarationEditorProps {
  /** The saved declaration for this model; `null` when there is none. */
  value: VisionDeclaration | null;
  /** What applies with no declaration (a cloud provider's default); `null` when the model cannot see. */
  defaultGeometry: VisionGeometry | null;
  onChange: (next: VisionDeclaration) => void;
  /** Says whose declaration this is when a row holds several. */
  modelName?: string;
}

export function VisionDeclarationEditor({ value, defaultGeometry, onChange, modelName }: VisionDeclarationEditorProps) {
  const id = useId();
  const effective = value ?? (defaultGeometry ? { enabled: true, geometry: defaultGeometry } : null);
  const enabled = effective?.enabled ?? false;
  const geometry = effective?.geometry ?? defaultGeometry ?? CONSERVATIVE_DEFAULT;
  // "Custom" stays chosen while the user edits, even when the numbers happen
  // to match a preset.
  const [pinnedCustom, setPinnedCustom] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);

  const choice: Choice = !enabled ? 'off'
    : pinnedCustom ? 'custom'
      : sameGeometry(geometry, CONSERVATIVE_DEFAULT) ? 'conservative'
        : presetOf(geometry) ?? 'custom';

  const choose = (next: Choice) => {
    setPinnedCustom(next === 'custom');
    if (next === 'off') {
      onChange({ enabled: false, geometry });
    } else if (next === 'custom') {
      setShowAdvanced(true);
      onChange({ enabled: true, geometry });
    } else if (next === 'conservative') {
      onChange({ enabled: true, geometry: CONSERVATIVE_DEFAULT });
    } else {
      onChange({ enabled: true, geometry: presetGeometry(next) });
    }
  };

  const editGeometry = (next: VisionGeometry) => {
    setPinnedCustom(true);
    onChange({ enabled: true, geometry: next });
  };

  const preview = planPicture(geometry, PREVIEW_SIZE.width, PREVIEW_SIZE.height);
  const selectLabel = modelName ? `Vision for ${modelName}` : 'Vision';

  return (
    <div className="vision-declaration min-w-0">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <label htmlFor={`${id}-choice`} className="text-xs font-medium text-text-secondary">
          Vision
          {modelName ? <span className="ml-1.5 font-mono font-normal text-text-muted">{modelName}</span> : null}
        </label>
        <select
          id={`${id}-choice`}
          aria-label={selectLabel}
          value={choice}
          onChange={(event) => choose(event.target.value as Choice)}
          className={cn(INPUT_CLASS, 'h-7 w-auto max-w-full py-0 text-xs')}
        >
          <option value="off">Off</option>
          <option value="conservative">{CONSERVATIVE_LABEL}</option>
          {VISION_PRESETS.map((preset) => (
            <option key={preset.id} value={preset.id}>{preset.label}</option>
          ))}
          <option value="custom">Custom</option>
        </select>
        {enabled ? (
          <button
            type="button"
            onClick={() => setShowAdvanced((open) => !open)}
            aria-expanded={showAdvanced}
            aria-controls={`${id}-advanced`}
            className="inline-flex items-center gap-0.5 text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary"
          >
            <ChevronRight className={cn('h-3 w-3 transition-transform duration-fast', showAdvanced && 'rotate-90')} aria-hidden="true" />
            Advanced
          </button>
        ) : null}
      </div>
      <p className="mt-1 text-xs text-text-muted" data-testid="vision-cost-preview">
        {enabled
          ? `A ${PREVIEW_SIZE.width}×${PREVIEW_SIZE.height} photo costs about ${formatCount(preview.tokens)} tokens on this model.`
          : 'Pictures are not shown to this model.'}
      </p>
      {enabled && showAdvanced ? (
        <GeometryFields id={`${id}-advanced`} geometry={geometry} onCommit={editGeometry} />
      ) : null}
    </div>
  );
}

function GeometryFields({ id, geometry, onCommit }: { id: string; geometry: VisionGeometry; onCommit: (next: VisionGeometry) => void }) {
  const fields = FIELDS[geometry.kind];
  const values = geometry as unknown as Record<string, number>;
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [problem, setProblem] = useState<string | null>(null);

  // A saved change (or another preset) replaces whatever was being typed.
  useEffect(() => {
    setDrafts({});
    setProblem(null);
  }, [geometry]);

  const commit = (key: string) => {
    const raw = drafts[key];
    if (raw === undefined) return;
    const parsed = Number(raw.trim());
    if (raw.trim() === '' || !Number.isInteger(parsed) || parsed < 0 || parsed > 0xffff_ffff) {
      setProblem('Use a whole number of 0 or more.');
      return;
    }
    if (parsed === values[key]) {
      setDrafts((previous) => Object.fromEntries(Object.entries(previous).filter(([name]) => name !== key)));
      setProblem(null);
      return;
    }
    const next = { ...geometry, [key]: parsed } as VisionGeometry;
    const why = geometryProblem(next);
    if (why) {
      setProblem(`Not saved: ${why}.`);
      return;
    }
    setProblem(null);
    onCommit(next);
  };

  return (
    <div id={id} className="mt-2 rounded-md border border-border-subtle bg-surface-sunken p-3">
      <div role="radiogroup" aria-label="Geometry kind" className="mb-3 inline-flex gap-1 rounded-md border border-border-subtle bg-surface p-0.5">
        {KINDS.map(({ kind, label, template }) => {
          const active = geometry.kind === kind;
          return (
            <button
              key={kind}
              type="button"
              role="radio"
              aria-checked={active}
              onClick={() => { if (!active) onCommit(template); }}
              className={cn(
                'rounded-sm px-2.5 py-0.5 text-xs transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                active ? 'bg-accent-muted font-medium text-accent' : 'text-text-secondary hover:text-text-primary',
              )}
            >
              {label}
            </button>
          );
        })}
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(8.5rem,1fr))] gap-x-3 gap-y-2">
        {fields.map((field) => (
          <label key={field.key} className="flex min-w-0 flex-col gap-1 text-xs text-text-muted">
            {field.label}
            <input
              type="number"
              inputMode="numeric"
              min={0}
              step={1}
              value={drafts[field.key] ?? String(values[field.key])}
              onChange={(event) => setDrafts((previous) => ({ ...previous, [field.key]: event.target.value }))}
              onBlur={() => commit(field.key)}
              onKeyDown={(event) => { if (event.key === 'Enter') commit(field.key); }}
              className={cn(NUMBER_FIELD_CLASS, 'h-7 w-full text-xs')}
            />
          </label>
        ))}
      </div>
      {problem ? <p role="alert" className="mt-2 text-xs text-danger-fg">{problem}</p> : null}
    </div>
  );
}

/**
 * The declaration for one model, read from and saved to LLM settings under
 * the model's reference key (`llamacpp:<model>`, `ollama:<tag>`,
 * `cloud:<provider>/<model>`).
 */
export function ModelVisionDeclaration({ modelRef, modelName }: { modelRef: ModelRef; modelName?: string }) {
  const { llmSettings, saveLlmUpdates } = useLlmSettings();
  const declarations = llmSettings?.visionDeclarations ?? {};
  const key = modelRefKey(modelRef);
  const defaultGeometry = modelRef.kind === 'cloud' ? cloudVisionDefault(modelRef.provider, modelRef.model) : null;

  return (
    <VisionDeclarationEditor
      value={declarations[key] ?? null}
      defaultGeometry={defaultGeometry}
      modelName={modelName}
      onChange={(next) => void saveLlmUpdates({ visionDeclarations: { ...declarations, [key]: next } })}
    />
  );
}
