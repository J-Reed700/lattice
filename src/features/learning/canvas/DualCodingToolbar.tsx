import { useCallback, useId, useMemo, useState } from 'react';

import { Brain, Check, ChevronDown, Columns2, Copy, FileText, GitCompare, GitFork, Network, Sparkles, X } from 'lucide-react';

import { MermaidFigure } from '@/components/TiptapEditor/mermaid/MermaidFigure';
import { canvasTextOutline, type CanvasScene } from '@/features/learning/canvas/canvasScene';
import {
  DUAL_CODING_TEMPLATES,
  buildDualCodingSceneElements,
  buildLessonMermaidDiagram,
  type DualCodingTemplateId,
} from '@/features/learning/canvas/dualCodingTemplates';

export interface LessonBlockItem {
  id: string;
  title: string;
  body: string;
}

export interface DualCodingToolbarProps {
  lessonTitle: string;
  lessonObjective?: string;
  lessonBlocks?: LessonBlockItem[];
  onApplyTemplate: (elementsJson: string, outline: string[], elementCount: number) => void;
}

const TEMPLATE_ICONS: Record<DualCodingTemplateId, typeof Network> = {
  concept_map: Network,
  compare_contrast: GitCompare,
  process_flow: GitFork,
  visual_analogy: Columns2,
};

export function DualCodingToolbar({
  lessonTitle,
  lessonObjective,
  lessonBlocks,
  onApplyTemplate,
}: DualCodingToolbarProps) {
  const drawerId = useId();
  const [referenceOpen, setReferenceOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<'text' | 'mermaid'>('text');
  const [copied, setCopied] = useState(false);
  const [appliedTemplate, setAppliedTemplate] = useState<string | null>(null);

  const keyPoints = useMemo(
    () => (lessonBlocks ?? []).map((b) => b.title).filter((t) => t && t.trim().length > 0),
    [lessonBlocks],
  );

  const mermaidSource = useMemo(
    () => buildLessonMermaidDiagram(lessonTitle, keyPoints),
    [lessonTitle, keyPoints],
  );

  const handleApply = useCallback((templateId: DualCodingTemplateId) => {
    const elements = buildDualCodingSceneElements(templateId, lessonTitle, keyPoints);
    const scene: CanvasScene = {
      type: 'excalidraw',
      version: 2,
      source: 'Lattice Learning Studio',
      elements: [...elements],
      appState: { viewBackgroundColor: '#ffffff', theme: 'light' },
      files: {},
    };
    const sceneJson = JSON.stringify(scene);
    const outline = canvasTextOutline(elements);
    onApplyTemplate(sceneJson, outline, elements.length);

    const definition = DUAL_CODING_TEMPLATES.find((t) => t.id === templateId);
    setAppliedTemplate(definition?.label ?? 'Template');
    const timer = setTimeout(() => setAppliedTemplate(null), 2500);
    return () => clearTimeout(timer);
  }, [lessonTitle, keyPoints, onApplyTemplate]);

  const handleCopyMermaid = useCallback(async () => {
    try {
      const markdownCode = `\`\`\`mermaid\n${mermaidSource}\n\`\`\``;
      await navigator.clipboard.writeText(markdownCode);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Graceful fallback for non-clipboard environments
    }
  }, [mermaidSource]);

  return (
    <div className="mb-4 rounded-xl border border-border-default bg-surface px-3.5 py-2.5 shadow-2xs">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-1.5 sm:gap-2">
          <div className="inline-flex items-center gap-1.5 rounded-lg border border-border-subtle bg-surface-sunken px-2.5 py-1 text-xs font-semibold text-text-primary">
            <span className="flex h-4 w-4 items-center justify-center rounded bg-accent/15 text-accent">
              <Brain size={12} />
            </span>
            <span>Dual Coding</span>
          </div>

          <div className="hidden h-4 w-px bg-border-subtle sm:block" />

          {DUAL_CODING_TEMPLATES.map((tmpl) => {
            const IconComponent = TEMPLATE_ICONS[tmpl.id];
            return (
              <button
                key={tmpl.id}
                type="button"
                title={tmpl.description}
                onClick={() => handleApply(tmpl.id)}
                className="inline-flex items-center gap-1.5 rounded-lg border border-border-default bg-surface px-2.5 py-1 text-xs font-medium text-text-secondary transition-colors hover:border-accent hover:bg-accent/10 hover:text-accent"
              >
                <IconComponent size={13} className="text-accent" />
                <span>{tmpl.label}</span>
                <span className="rounded bg-surface-sunken px-1.5 py-0.5 text-[9px] text-text-muted">
                  {tmpl.badge}
                </span>
              </button>
            );
          })}

          {appliedTemplate && (
            <span role="status" className="inline-flex items-center gap-1 text-[11px] font-medium text-success-fg">
              <Sparkles size={12} /> Applied {appliedTemplate} to canvas
            </span>
          )}
        </div>

        <button
          type="button"
          aria-expanded={referenceOpen}
          aria-controls={drawerId}
          onClick={() => setReferenceOpen((prev) => !prev)}
          className={`inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs font-medium transition-colors ${
            referenceOpen
              ? 'border-accent bg-accent/10 text-accent'
              : 'border-border-default bg-surface text-text-secondary hover:border-accent/40 hover:text-text-primary'
          }`}
        >
          <FileText size={12} />
          Side-by-Side Lesson Text &amp; Mermaid Preview
          <ChevronDown size={12} className={`transition-transform duration-200 ${referenceOpen ? 'rotate-180' : ''}`} />
        </button>
      </div>

      <details className="mt-2 border-t border-border-subtle/60 pt-1.5 text-[11px] text-text-muted">
        <summary className="cursor-pointer select-none text-text-muted transition-colors hover:text-text-secondary">
          Dual Coding &amp; Visualization rationale
        </summary>
        <p className="mt-1 text-text-muted">
          Pair dense text or abstract numbers with charts, diagrams, or mental imagery. Visual information is processed and stored in long-term memory far more effectively than raw text alone.
        </p>
      </details>

      {referenceOpen && (
        <section
          id={drawerId}
          aria-label="Side-by-Side Lesson Text & Mermaid Preview"
          className="mt-3 border-t border-border-default pt-3"
        >
          <div className="mb-3 flex items-center justify-between border-b border-border-subtle pb-2.5">
            <div className="flex items-center gap-2">
              <button
                type="button"
                aria-pressed={activeTab === 'text'}
                onClick={() => setActiveTab('text')}
                className={`inline-flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs font-semibold ${
                  activeTab === 'text'
                    ? 'bg-accent text-accent-fg'
                    : 'text-text-secondary hover:bg-surface-sunken hover:text-text-primary'
                }`}
              >
                <FileText size={13} />
                Lesson Reference Text ({lessonBlocks?.length ?? 0} blocks)
              </button>
              <button
                type="button"
                aria-pressed={activeTab === 'mermaid'}
                onClick={() => setActiveTab('mermaid')}
                className={`inline-flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs font-semibold ${
                  activeTab === 'mermaid'
                    ? 'bg-accent text-accent-fg'
                    : 'text-text-secondary hover:bg-surface-sunken hover:text-text-primary'
                }`}
              >
                <Network size={13} />
                Mermaid Concept Flowchart Preview
              </button>
            </div>

            <div className="flex items-center gap-2">
              {activeTab === 'mermaid' && (
                <button
                  type="button"
                  onClick={() => void handleCopyMermaid()}
                  className="inline-flex items-center gap-1 rounded-md border border-border-default bg-surface px-2.5 py-1 text-xs text-text-secondary hover:bg-surface-sunken"
                >
                  {copied ? <Check size={12} className="text-success-fg" /> : <Copy size={12} />}
                  {copied ? 'Copied to clipboard' : 'Copy Mermaid for Notebook'}
                </button>
              )}
              <button
                type="button"
                aria-label="Close reference drawer"
                onClick={() => setReferenceOpen(false)}
                className="rounded-md p-1 text-text-muted hover:bg-surface-sunken hover:text-text-primary"
              >
                <X size={14} />
              </button>
            </div>
          </div>

          {activeTab === 'text' ? (
            <div className="max-h-80 space-y-3 overflow-y-auto pr-1">
              {lessonObjective && (
                <div className="rounded-lg border border-accent/20 bg-accent/10 p-3">
                  <div className="text-[10px] font-semibold uppercase tracking-[.12em] text-accent">
                    Lesson Objective
                  </div>
                  <p className="mt-1 text-xs leading-5 text-text-primary">{lessonObjective}</p>
                </div>
              )}

              {lessonBlocks && lessonBlocks.length > 0 ? (
                lessonBlocks.map((block, idx) => (
                  <article
                    key={block.id || `block-${idx}`}
                    className="rounded-lg border border-border-default bg-surface-sunken/40 p-3"
                  >
                    <h4 className="text-xs font-semibold text-text-primary">{block.title}</h4>
                    <p className="mt-1.5 whitespace-pre-wrap text-xs leading-relaxed text-text-secondary">
                      {block.body}
                    </p>
                  </article>
                ))
              ) : (
                <p className="text-xs text-text-muted">
                  No lesson text blocks attached to this canvas. You can sketch ideas freely using the dual coding templates.
                </p>
              )}
            </div>
          ) : (
            <div className="space-y-3">
              <p className="text-xs text-text-secondary">
                This diagram is automatically generated from the lesson&apos;s conceptual hierarchy. Copy the Mermaid syntax to embed it into your personal Notebook or notes.
              </p>
              <div className="overflow-hidden rounded-lg border border-border-default bg-surface p-3">
                <MermaidFigure source={mermaidSource}>
                  <pre
                    data-testid="mermaid-code-preview"
                    className="overflow-x-auto rounded-lg bg-surface-sunken p-3 font-mono text-[11px] leading-relaxed text-text-primary"
                  >
                    {mermaidSource}
                  </pre>
                </MermaidFigure>
              </div>
            </div>
          )}
        </section>
      )}
    </div>
  );
}

export default DualCodingToolbar;
