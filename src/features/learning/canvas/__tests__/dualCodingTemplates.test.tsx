import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { canvasTextOutline, parseCanvasScene, type CanvasScene } from '@/features/learning/canvas/canvasScene';
import {
  DUAL_CODING_TEMPLATES,
  buildDualCodingSceneElements,
  buildLessonMermaidDiagram,
  type DualCodingTemplateId,
} from '@/features/learning/canvas/dualCodingTemplates';
import DualCodingToolbar from '@/features/learning/canvas/DualCodingToolbar';

vi.mock('@excalidraw/excalidraw', () => ({
  serializeAsJSON: (elements: unknown[], _state: unknown, _files: unknown, source: string) =>
    JSON.stringify({ type: 'excalidraw', version: 2, source, elements, appState: {}, files: {} }),
  exportToBlob: vi.fn(),
  exportToSvg: vi.fn(),
}));

vi.mock('@/components/TiptapEditor/mermaid/MermaidFigure', () => ({
  MermaidFigure: ({ source, children }: { source: string; children: React.ReactNode }) => (
    <div data-testid="mermaid-figure-mock">
      <div data-testid="mock-mermaid-svg">{source}</div>
      {children}
    </div>
  ),
}));

describe('Dual Coding Visual Templates', () => {
  const sampleTitle = 'Photosynthesis & Carbon Fixation';
  const samplePoints = [
    'Light-Dependent Reactions',
    'Thylakoid Membrane Gradient',
    'Calvin-Benson Cycle',
    'RuBisCO Carbon Fixation',
  ];

  it('defines 4 structured dual coding templates', () => {
    expect(DUAL_CODING_TEMPLATES).toHaveLength(4);
    const ids = DUAL_CODING_TEMPLATES.map((t) => t.id);
    expect(ids).toEqual(['concept_map', 'compare_contrast', 'process_flow', 'visual_analogy']);
  });

  const templateIds: DualCodingTemplateId[] = [
    'concept_map',
    'compare_contrast',
    'process_flow',
    'visual_analogy',
  ];

  it.each(templateIds)('generates valid Excalidraw elements for template: %s', (templateId) => {
    const elements = buildDualCodingSceneElements(templateId, sampleTitle, samplePoints);
    expect(elements.length).toBeGreaterThan(5);

    // Validate Excalidraw element contract for canvas_repository.rs and Excalidraw
    for (const elem of elements) {
      expect(typeof elem.id).toBe('string');
      expect(elem.id.trim()).not.toBe('');
      expect(typeof elem.type).toBe('string');
      expect(['rectangle', 'ellipse', 'diamond', 'arrow', 'text']).toContain(elem.type);
      expect(elem.isDeleted).toBe(false);
      expect(elem.version).toBe(1);
      expect(typeof elem.seed).toBe('number');
      expect(elem.type).not.toBe('image');
      expect(elem.type).not.toBe('embeddable');
      expect(elem.type).not.toBe('iframe');
    }

    // Wrap in CanvasScene and test parseCanvasScene validation
    const scene: CanvasScene = {
      type: 'excalidraw',
      version: 2,
      source: 'Lattice Learning Studio',
      elements: [...elements],
      appState: { viewBackgroundColor: '#ffffff' },
      files: {},
    };
    const sceneJson = JSON.stringify(scene);
    const parsed = parseCanvasScene(sceneJson);
    expect(parsed.elements.length).toBe(elements.length);
    expect(parsed.type).toBe('excalidraw');

    // Validate canvasTextOutline extracts meaningful text labels and shape names
    const outline = canvasTextOutline(elements);
    expect(outline.length).toBeGreaterThan(0);
    // At least one outline entry contains part of the title or key points
    const hasRecognizableLabel = outline.some(
      (line) => line.includes('Photosynthesis') || line.includes('Reactions') || line.includes('Core') || line.includes('Dual Coding'),
    );
    expect(hasRecognizableLabel).toBe(true);
  });

  it('buildLessonMermaidDiagram produces valid Mermaid syntax with title and concept nodes', () => {
    const diagram = buildLessonMermaidDiagram(sampleTitle, samplePoints);
    expect(diagram).toContain('flowchart TD');
    expect(diagram).toContain('Core["Photosynthesis & Carbon Fixation"]');
    expect(diagram).toContain('Node_1["Light-Dependent Reactions"]');
    expect(diagram).toContain('Node_2["Thylakoid Membrane Gradient"]');
    expect(diagram).toContain('Core --> Node_1');
    expect(diagram).toContain('Node_1 -.-> Node_2');
  });

  it('buildLessonMermaidDiagram falls back gracefully when title or points are empty', () => {
    const fallbackDiagram = buildLessonMermaidDiagram('', []);
    expect(fallbackDiagram).toContain('flowchart TD');
    expect(fallbackDiagram).toContain('Core["Lesson Overview"]');
    expect(fallbackDiagram).toContain('Node_1["Core Principle & Foundation"]');
  });
});

describe('DualCodingToolbar component', () => {
  const onApplyTemplate = vi.fn();
  const sampleBlocks = [
    { id: 'b1', title: 'Light Energy Harvesting', body: 'Photons excite electrons in chlorophyll P680 and P700.' },
    { id: 'b2', title: 'Proton Gradient & ATP', body: 'Cytochrome b6f pumps protons to generate ATP synthase torque.' },
    { id: 'b3', title: 'Dark Reactions & Sugar', body: 'The Calvin cycle consumes ATP and NADPH to reduce CO2 into G3P.' },
  ];

  const writeTextMock = vi.fn().mockResolvedValue(undefined);

  beforeEach(() => {
    onApplyTemplate.mockReset();
    writeTextMock.mockReset();
    writeTextMock.mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: {
        writeText: writeTextMock,
      },
      configurable: true,
      writable: true,
    });
  });

  it('renders dual coding cognitive explanation banner and template buttons', () => {
    render(
      <DualCodingToolbar
        lessonTitle="Cellular Bioenergetics"
        lessonObjective="Understand photosynthetic electron transport and carbon assimilation."
        lessonBlocks={sampleBlocks}
        onApplyTemplate={onApplyTemplate}
      />,
    );

    expect(screen.getByText('Dual Coding')).toBeVisible();
    expect(screen.getByText('Dual Coding & Visualization rationale')).toBeVisible();
    expect(
      screen.getByText(/Pair dense text or abstract numbers with charts, diagrams, or mental imagery/),
    ).toBeInTheDocument();

    expect(screen.getByRole('button', { name: /Concept Map/ })).toBeVisible();
    expect(screen.getByRole('button', { name: /Compare & Contrast/ })).toBeVisible();
    expect(screen.getByRole('button', { name: /Process Flowchart/ })).toBeVisible();
    expect(screen.getByRole('button', { name: /Mental Imagery Split/ })).toBeVisible();
  });

  it('triggers onApplyTemplate with valid scene JSON, outline, and element count on click', async () => {
    const user = userEvent.setup();
    render(
      <DualCodingToolbar
        lessonTitle="Cellular Bioenergetics"
        lessonObjective="Understand photosynthetic electron transport."
        lessonBlocks={sampleBlocks}
        onApplyTemplate={onApplyTemplate}
      />,
    );

    const conceptMapBtn = screen.getByRole('button', { name: /Concept Map/ });
    await user.click(conceptMapBtn);

    expect(onApplyTemplate).toHaveBeenCalledTimes(1);
    const [sceneJson, outline, elementCount] = onApplyTemplate.mock.calls[0] as [string, string[], number];

    expect(elementCount).toBeGreaterThan(5);
    expect(outline.length).toBeGreaterThan(0);

    const parsed = parseCanvasScene(sceneJson);
    expect(parsed.elements.length).toBe(elementCount);

    expect(screen.getByText(/Applied Concept Map to canvas/)).toBeVisible();
  });

  it('toggles side-by-side reference drawer and displays lesson text and objective', async () => {
    const user = userEvent.setup();
    render(
      <DualCodingToolbar
        lessonTitle="Cellular Bioenergetics"
        lessonObjective="Master ATP synthesis mechanisms."
        lessonBlocks={sampleBlocks}
        onApplyTemplate={onApplyTemplate}
      />,
    );

    const toggleBtn = screen.getByRole('button', { name: /Side-by-Side Lesson Text & Mermaid Preview/ });
    expect(screen.queryByLabelText('Side-by-Side Lesson Text & Mermaid Preview')).not.toBeInTheDocument();

    await user.click(toggleBtn);
    const drawer = screen.getByLabelText('Side-by-Side Lesson Text & Mermaid Preview');
    expect(drawer).toBeVisible();

    expect(screen.getByText('Master ATP synthesis mechanisms.')).toBeVisible();
    expect(screen.getByText('Light Energy Harvesting')).toBeVisible();
    expect(screen.getByText(/Photons excite electrons/)).toBeVisible();
    expect(screen.getByText('Proton Gradient & ATP')).toBeVisible();
  });

  it('switches to Mermaid preview tab and allows copying Mermaid syntax for Notebook', async () => {
    const user = userEvent.setup();
    const writeSpy = vi.spyOn(navigator.clipboard, 'writeText');
    render(
      <DualCodingToolbar
        lessonTitle="Cellular Bioenergetics"
        lessonObjective="Master ATP synthesis mechanisms."
        lessonBlocks={sampleBlocks}
        onApplyTemplate={onApplyTemplate}
      />,
    );

    await user.click(screen.getByRole('button', { name: /Side-by-Side Lesson Text & Mermaid Preview/ }));

    const mermaidTabBtn = screen.getByRole('button', { name: /Mermaid Concept Flowchart Preview/ });
    await user.click(mermaidTabBtn);

    const codePreview = screen.getByTestId('mermaid-code-preview');
    expect(codePreview).toHaveTextContent('flowchart TD');
    expect(codePreview).toHaveTextContent('Cellular Bioenergetics');

    const copyBtn = screen.getByRole('button', { name: /Copy Mermaid for Notebook/ });
    await user.click(copyBtn);

    expect(writeSpy).toHaveBeenCalledWith(
      expect.stringContaining('```mermaid\nflowchart TD'),
    );
    expect(await screen.findByText('Copied to clipboard')).toBeVisible();

    const closeBtn = screen.getByRole('button', { name: 'Close reference drawer' });
    await user.click(closeBtn);
    expect(screen.queryByLabelText('Side-by-Side Lesson Text & Mermaid Preview')).not.toBeInTheDocument();
  });
});
