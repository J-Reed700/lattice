import { useCallback, useEffect, useMemo, useState } from 'react';

import { useQuery } from '@tanstack/react-query';
import { Check, ExternalLink } from 'lucide-react';

import { modelApi } from '@/features/model/api/client';
import { cn } from '@/lib/utils';

import { computeModelFit, FIT_LABEL, formatSize } from './catalogUtils';
import { modelQuantization, quantizationDescription } from './quantization';
import { settingsFieldClass } from '../../ui';
import { SECONDARY_BUTTON_CLASS } from '../settingsStyles';

import type { SystemCapabilities } from '../../../types/api/models';
import type { ModelMetadata } from '../../../types/modelCatalog';

interface Props {
  model: ModelMetadata;
  selected: ModelMetadata;
  capabilities: SystemCapabilities | null;
  onSelect: (model: ModelMetadata) => void;
}

export function ModelVariantPicker({ model, selected, capabilities, onSelect }: Props) {
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState('size');
  const repoId = model.model_id!;
  const variants = useQuery({
    queryKey: ['model-catalog', 'variants', repoId],
    queryFn: async () => {
      const result = await modelApi.getModelVariants(repoId);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    staleTime: 5 * 60_000,
    retry: false,
  });
  const files = useMemo(() => (variants.data ?? [])
    .filter(file => `${file.default_filename} ${modelQuantization(file)}`.toLowerCase().includes(query.trim().toLowerCase()))
    .sort((a, b) => sort === 'name'
      ? (a.default_filename ?? '').localeCompare(b.default_filename ?? '')
      : (a.size_gb || Infinity) - (b.size_gb || Infinity)), [variants.data, query, sort]);
  const selectFile = useCallback((file: ModelMetadata) => {
    // Keep a curated default's identity so an already installed default does
    // not become a second download merely by opening the version chooser.
    onSelect(file.default_filename === model.default_filename ? { ...file, id: model.id } : file);
  }, [model.default_filename, model.id, onSelect]);

  useEffect(() => {
    // Hydrate the listed default with its reported size as soon as the file
    // list arrives, without overriding a version the user has already chosen.
    if (selected !== model) return;
    const published = variants.data?.find(file => file.default_filename === model.default_filename);
    if (published && published !== selected) selectFile(published);
  }, [model, selected, variants.data, selectFile]);

  return <section aria-label="Available versions" className="rounded-lg border border-border-subtle bg-surface-secondary/30 p-4">
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div>
        <h3 className="text-base font-medium text-text-primary">Choose a version</h3>
        <p className="mt-1 text-sm text-text-secondary">Same model, different weight precision and download size.</p>
      </div>
      <a href={`https://huggingface.co/${repoId}`} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-sm text-accent hover:underline">
        Publisher & model card <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
      </a>
    </div>
    <details className="mt-3 text-sm text-text-secondary">
      <summary className="cursor-pointer">What does quantization mean?</summary>
      <p className="mt-2 max-w-[75ch] leading-relaxed">Quantization stores model weights with less precision to reduce download size and memory use. Q4 and Q5 offer moderate compression; Q8 uses more space and preserves more precision. Smaller files can lose quality. Actual speed and accuracy depend on the model, runtime, and hardware.</p>
      <p className="mt-2">IQ and UD labels describe other quantization methods. Compare the publisher’s notes; these labels are not benchmark scores. <a className="text-accent hover:underline" href="https://huggingface.co/docs/hub/gguf" target="_blank" rel="noreferrer">Quantization reference</a></p>
    </details>
    {variants.isPending ? <p role="status" className="mt-4 text-sm text-text-secondary">Loading published files and download sizes from Hugging Face…</p> : variants.isError ? <div role="alert" className="mt-4 space-y-2">
      <p className="text-sm text-danger-fg">Couldn’t load versions. {variants.error.message}</p>
      <button className={SECONDARY_BUTTON_CLASS} type="button" onClick={() => void variants.refetch()}>Retry versions</button>
    </div> : <>
      <div className="my-4 flex flex-wrap items-end gap-3">
        <label className="min-w-0 flex-1 text-xs text-text-secondary">Filter versions
          <input value={query} onChange={event => setQuery(event.target.value)} placeholder="e.g. Q4_K_M, Q8, BF16" className={cn(settingsFieldClass, 'mt-1')} />
        </label>
        <label className="text-xs text-text-secondary">Order versions
          <select value={sort} onChange={event => setSort(event.target.value)} className={cn(settingsFieldClass, 'mt-1')}>
            <option value="size">Smallest download</option><option value="name">Filename</option>
          </select>
        </label>
      </div>
      <p role="status" className="mb-2 text-xs text-text-muted">{files.length} of {variants.data?.length ?? 0} standalone versions</p>
      <div role="group" aria-label="Model files" className="max-h-80 space-y-2 overflow-y-auto pr-1">
        {files.map(file => {
          const quantization = modelQuantization(file);
          const fit = computeModelFit(file, capabilities);
          const chosen = selected.default_filename === file.default_filename;
          return <button key={file.id} type="button" aria-pressed={chosen} onClick={() => selectFile(file)} className={cn('w-full rounded-md border p-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring', chosen ? 'border-accent bg-accent/5' : 'border-border-subtle hover:bg-surface-hover')}>
            <span className="flex flex-wrap items-center justify-between gap-2 text-sm font-medium text-text-primary">
              <span className="inline-flex items-center gap-2">{chosen ? <Check className="h-4 w-4 text-accent" aria-hidden="true" /> : null}{quantization ?? 'GGUF'}{chosen ? <span className="text-xs text-accent">Selected</span> : null}</span>
              <span className="tabular-nums">{formatSize(file.size_gb) ?? 'Size unavailable'}</span>
            </span>
            <span className="mt-1 block text-xs text-text-secondary">{quantizationDescription(quantization)}</span>
            <span className="mt-1 block break-all font-mono text-xs text-text-muted">{file.default_filename}</span>
            <span className="mt-2 block text-xs text-text-secondary">{file.minimum_ram_gb > 0 ? `Est. memory ${file.minimum_ram_gb.toFixed(1)} GB` : 'Memory estimate unavailable'}{fit ? ` · ${FIT_LABEL[fit.verdict]} on this computer` : ''}</span>
          </button>;
        })}
      </div>
      {files.length === 0 ? <p className="mt-3 text-sm text-text-secondary">{query ? 'No versions match this filter.' : 'No standalone GGUF downloads are available in this repository.'}</p> : null}
    </>}
    <p className="mt-3 text-xs leading-relaxed text-text-muted">Sizes are reported by the publisher. Memory and fit are estimates; context length and other running models need additional memory. This picker supports standalone GGUF files at the repository root. Split files, subfolders, and vision projectors are excluded.</p>
  </section>;
}
