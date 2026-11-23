import { useCallback, useEffect, useId, useMemo, useState } from 'react';

import { Check, ExternalLink } from 'lucide-react';

import { Button, Input } from '@/components/ui';
import { useShownFits } from '@/features/model/hooks/useModelFits';
import { useModelVariantsQuery } from '@/features/model/hooks/useModelVariantsQuery';
import { fitLabel } from '@/features/model/utils/fitText';
import { cn } from '@/lib/utils';
import type { ModelMetadata } from '@/types/modelCatalog';

import { formatSize } from './catalogUtils';
import { modelQuantization, quantizationDescription } from './quantization';
import { SettingsSelect } from '../SettingControls';

interface Props {
  model: ModelMetadata;
  selected: ModelMetadata;
  onSelect: (model: ModelMetadata) => void;
}

const ORDER_OPTIONS = [
  { value: 'size', label: 'Smallest download' },
  { value: 'name', label: 'Filename' },
];

export function ModelVariantPicker({ model, selected, onSelect }: Props) {
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState('size');
  const baseId = useId();
  const repoId = model.model_id!;
  const variants = useModelVariantsQuery(repoId);
  const files = useMemo(
    () =>
      (variants.data ?? [])
        .filter((file) =>
          `${file.default_filename} ${modelQuantization(file)}`
            .toLowerCase()
            .includes(query.trim().toLowerCase()),
        )
        .sort((a, b) =>
          sort === 'name'
            ? (a.default_filename ?? '').localeCompare(b.default_filename ?? '')
            : (a.size_gb || Infinity) - (b.size_gb || Infinity),
        ),
    [variants.data, query, sort],
  );
  const shownFits = useShownFits(files.map((file) => ({ model: file, fit: file.fit })));
  const selectFile = useCallback(
    (file: ModelMetadata) => {
      onSelect(file.default_filename === model.default_filename ? { ...file, id: model.id } : file);
    },
    [model.default_filename, model.id, onSelect],
  );

  useEffect(() => {
    if (selected !== model) return;
    const published = variants.data?.find(
      (file) => file.default_filename === model.default_filename,
    );
    if (published && published !== selected) selectFile(published);
  }, [model, selected, variants.data, selectFile]);

  return (
    <section
      aria-label="Available versions"
      className="rounded-xl border border-border-subtle bg-surface-sunken/50 p-4"
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="text-base font-medium text-text-primary">Choose a version</h3>
          <p className="mt-1 text-sm text-text-secondary">
            Same model, different weight precision and download size.
          </p>
        </div>
        <a
          href={`https://huggingface.co/${repoId}`}
          target="_blank"
          rel="noreferrer"
          className="inline-flex items-center gap-1 text-sm text-accent hover:underline"
        >
          Publisher &amp; model card <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
        </a>
      </div>
      <details className="mt-3 text-sm text-text-secondary">
        <summary className="cursor-pointer">What does quantization mean?</summary>
        <p className="mt-2 max-w-[75ch] leading-relaxed">
          Quantization stores model weights with less precision to reduce download size and memory
          use. Q4 and Q5 offer moderate compression; Q8 uses more space and preserves more precision.
          Smaller files can lose quality. Actual speed and accuracy depend on the model, runtime,
          and hardware.
        </p>
        <p className="mt-2">
          IQ and UD labels describe other quantization methods. Compare the publisher’s notes; these
          labels are not benchmark scores.{' '}
          <a
            className="text-accent hover:underline"
            href="https://huggingface.co/docs/hub/gguf"
            target="_blank"
            rel="noreferrer"
          >
            Quantization reference
          </a>
        </p>
      </details>
      {variants.isPending ? (
        <p role="status" className="mt-4 text-sm text-text-secondary">
          Loading published files and download sizes from Hugging Face…
        </p>
      ) : variants.isError ? (
        <div role="alert" className="mt-4 space-y-2">
          <p className="text-sm text-danger-fg">Couldn’t load versions. {variants.error.message}</p>
          <Button type="button" variant="outline" onClick={() => void variants.refetch()}>
            Retry versions
          </Button>
        </div>
      ) : (
        <>
          <div className="my-4 flex flex-wrap items-end gap-3">
            <div className="min-w-0 flex-1">
              <Input
                id={`${baseId}-filter`}
                label="Filter versions"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="e.g. Q4_K_M, Q8, BF16"
              />
            </div>
            <div className="w-44 shrink-0 space-y-1">
              <label htmlFor={`${baseId}-order`} className="block text-xs font-medium text-text-secondary">
                Order versions
              </label>
              <SettingsSelect
                id={`${baseId}-order`}
                value={sort}
                onValueChange={setSort}
                options={ORDER_OPTIONS}
              />
            </div>
          </div>
          <p role="status" className="mb-2 text-xs text-text-muted">
            {files.length} of {variants.data?.length ?? 0} standalone versions
          </p>
          <div role="group" aria-label="Model files" className="max-h-80 space-y-2 overflow-y-auto pr-1">
            {files.map((file, index) => {
              const quantization = modelQuantization(file);
              const fit = shownFits[index];
              const chosen = selected.default_filename === file.default_filename;
              return (
                <button
                  key={file.id}
                  type="button"
                  aria-pressed={chosen}
                  onClick={() => selectFile(file)}
                  className={cn(
                    'w-full rounded-lg border p-3 text-left transition-colors focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                    chosen
                      ? 'border-accent bg-accent-muted'
                      : 'border-border-subtle bg-surface hover:border-border-strong',
                  )}
                >
                  <span className="flex flex-wrap items-center justify-between gap-2 text-sm font-medium text-text-primary">
                    <span className="inline-flex items-center gap-2">
                      {chosen ? <Check className="h-4 w-4 text-accent" aria-hidden="true" /> : null}
                      {quantization ?? 'GGUF'}
                      {chosen ? <span className="text-xs text-accent">Selected</span> : null}
                    </span>
                    <span className="tabular-nums">
                      {formatSize(file.size_gb) ?? 'Size unavailable'}
                    </span>
                  </span>
                  <span className="mt-1 block text-xs text-text-secondary">
                    {quantizationDescription(quantization)}
                  </span>
                  <span className="mt-1 block break-all font-mono text-xs text-text-muted">
                    {file.default_filename}
                  </span>
                  {fit !== undefined ? (
                    <span className="mt-2 block text-xs text-text-secondary">{fitLabel(fit)}</span>
                  ) : null}
                </button>
              );
            })}
          </div>
          {files.length === 0 ? (
            <p className="mt-3 text-sm text-text-secondary">
              {query
                ? 'No versions match this filter.'
                : 'No standalone GGUF downloads are available in this repository.'}
            </p>
          ) : null}
        </>
      )}
      <p className="mt-3 text-xs leading-relaxed text-text-muted">
        Sizes are reported by the publisher. Memory and fit are estimates; context length and other
        running models need additional memory. This picker supports standalone GGUF files at the
        repository root. Split files, subfolders, and vision projectors are excluded.
      </p>
    </section>
  );
}
