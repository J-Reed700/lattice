import { useId, useState, type KeyboardEvent } from 'react';

import { CodeViewer } from '@/features/explorer/components/CodeViewer';
import type { ExplorerLineRange } from '@/features/explorer/stores/explorerStore';
import { eyebrow, sectionHeading } from '@/features/learning/drills/reveal/revealStyle';
import type { CodeReviewFileDto } from '@/lib/bindings';

import '@/features/explorer/components/explorer.css';

/** The lines an issue points at, in the code reviewed and, when they can be found, in the clean code. */
export interface ContrastFocus {
  path: string;
  label: string;
  reviewed: ExplorerLineRange;
  clean: ExplorerLineRange | null;
  nonce: number;
}

function CodePane({ title, path, file, language, range, nonce }: {
  title: string;
  path: string;
  file: CodeReviewFileDto | undefined;
  language: string;
  range: ExplorerLineRange | null;
  nonce: number | null;
}) {
  const titleId = useId();
  const [picked, setPicked] = useState<ExplorerLineRange | null>(null);
  return (
    <div role="group" aria-labelledby={titleId} className="min-w-0 overflow-hidden rounded-2xl border border-border bg-bg">
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 border-b border-border px-4 py-2">
        <h3 id={titleId} className={eyebrow}>{title}</h3>
        <span className="break-all font-mono text-xs text-text-muted">{path}</span>
      </div>
      <div className="h-[34rem] max-h-[75vh]">
        {file ? (
          <CodeViewer
            path={file.path}
            text={file.text}
            language={language}
            selection={picked}
            highlight={range}
            highlightNonce={nonce}
            onSelectLines={setPicked}
          />
        ) : (
          <p className="px-4 py-3 text-sm text-text-muted">This version has no {path}.</p>
        )}
      </div>
    </div>
  );
}

function FileTabs({ paths, value, onChange, idBase, panelId }: {
  paths: readonly string[];
  value: string;
  onChange: (path: string) => void;
  idBase: string;
  panelId: string;
}) {
  const move = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const last = paths.length - 1;
    let target: number;
    switch (event.key) {
      case 'ArrowRight':
        target = (index + 1) % paths.length;
        break;
      case 'ArrowLeft':
        target = (index + last) % paths.length;
        break;
      case 'Home':
        target = 0;
        break;
      case 'End':
        target = last;
        break;
      default:
        return;
    }
    event.preventDefault();
    onChange(paths[target]);
    document.getElementById(`${idBase}-${target}`)?.focus();
  };
  return (
    <div role="tablist" aria-label="Files" className="flex flex-wrap gap-1">
      {paths.map((path, index) => {
        const selected = path === value;
        return (
          <button
            key={path}
            type="button"
            role="tab"
            id={`${idBase}-${index}`}
            aria-selected={selected}
            aria-controls={panelId}
            tabIndex={selected ? 0 : -1}
            onClick={() => onChange(path)}
            onKeyDown={(event) => move(event, index)}
            className={`max-w-full break-all rounded-lg px-3 py-2 font-mono text-xs transition focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent ${selected ? 'bg-surface text-accent shadow-sm' : 'text-text-secondary hover:bg-surface'}`}
          >
            {path}
          </button>
        );
      })}
    </div>
  );
}

/** The contrasting-cases step: the code the learner reviewed beside the same code without the issues. */
export function CodeContrast({ language, reviewed, clean, paths, path, onPathChange, focus }: {
  language: string;
  reviewed: readonly CodeReviewFileDto[];
  clean: readonly CodeReviewFileDto[];
  paths: readonly string[];
  path: string;
  onPathChange: (path: string) => void;
  focus: ContrastFocus | null;
}) {
  const headingId = useId();
  const tabsId = useId();
  const panelId = `${tabsId}-panel`;
  const tabbed = paths.length > 1;
  const active = focus?.path === path ? focus : null;
  const cleanRange = active?.clean ?? null;
  return (
    <section aria-labelledby={headingId} className="space-y-3">
      <div>
        <h2 id={headingId} className={sectionHeading}>The code you reviewed · The code without the issues</h2>
        <p className="mt-1 text-sm text-text-secondary">Choose an issue’s lines above to see them here, in both versions.</p>
      </div>
      {tabbed && <FileTabs paths={paths} value={path} onChange={onPathChange} idBase={tabsId} panelId={panelId} />}
      <div
        id={panelId}
        role={tabbed ? 'tabpanel' : undefined}
        aria-labelledby={tabbed ? `${tabsId}-${paths.indexOf(path)}` : undefined}
        className="grid gap-4 lg:grid-cols-2"
      >
        <CodePane
          key={`reviewed:${path}`}
          title="The code you reviewed"
          path={path}
          file={reviewed.find((file) => file.path === path)}
          language={language}
          range={active?.reviewed ?? null}
          nonce={active?.nonce ?? null}
        />
        <CodePane
          key={`clean:${path}`}
          title="The code without the issues"
          path={path}
          file={clean.find((file) => file.path === path)}
          language={language}
          range={cleanRange}
          nonce={cleanRange ? (active?.nonce ?? null) : null}
        />
      </div>
      <p role="status" className="sr-only">{active ? `Showing ${active.label}` : ''}</p>
    </section>
  );
}
