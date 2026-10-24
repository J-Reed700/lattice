import type { ExcalidrawElement } from '@excalidraw/excalidraw/element/types';

export type DualCodingTemplateId = 'concept_map' | 'compare_contrast' | 'process_flow' | 'visual_analogy';

export interface DualCodingTemplateDefinition {
  id: DualCodingTemplateId;
  label: string;
  description: string;
  badge: string;
}

export const DUAL_CODING_TEMPLATES: readonly DualCodingTemplateDefinition[] = [
  {
    id: 'concept_map',
    label: 'Concept Map',
    description: 'Central concept with radiating connected nodes for key principles and subtopics.',
    badge: 'Radial Organization',
  },
  {
    id: 'compare_contrast',
    label: 'Compare & Contrast',
    description: 'Two-column matrix comparing Concept A vs Concept B with shared core and key distinctions.',
    badge: 'Matrix Analysis',
  },
  {
    id: 'process_flow',
    label: 'Process Flowchart',
    description: 'Step-by-step linear workflow from initial setup through transformations to target outcome.',
    badge: 'Sequential Flow',
  },
  {
    id: 'visual_analogy',
    label: 'Mental Imagery Split',
    description: 'Dual coding layout pairing dense text & abstract logic on left with spatial diagram cues on right.',
    badge: 'Dual Coding Split',
  },
] as const;

interface ShapeOptions {
  id?: string;
  type: 'rectangle' | 'ellipse' | 'diamond';
  x: number;
  y: number;
  width: number;
  height: number;
  strokeColor?: string;
  backgroundColor?: string;
  strokeStyle?: 'solid' | 'dashed';
}

interface TextOptions {
  id?: string;
  text: string;
  x: number;
  y: number;
  width?: number;
  height?: number;
  fontSize?: number;
  textAlign?: 'left' | 'center' | 'right';
  strokeColor?: string;
}

function makeId(prefix = 'elem'): string {
  return `${prefix}_${Math.random().toString(36).slice(2, 10)}`;
}

function createShape(options: ShapeOptions): ExcalidrawElement {
  return {
    id: options.id ?? makeId(options.type),
    type: options.type,
    x: options.x,
    y: options.y,
    width: options.width,
    height: options.height,
    angle: 0,
    strokeColor: options.strokeColor ?? '#343a40',
    backgroundColor: options.backgroundColor ?? '#f8f9fa',
    fillStyle: 'solid',
    strokeWidth: 2,
    strokeStyle: options.strokeStyle ?? 'solid',
    roundness: options.type === 'rectangle' ? { type: 3 } : null,
    roughness: 1,
    opacity: 100,
    seed: Math.floor(Math.random() * 2_000_000_000),
    version: 1,
    versionNonce: Math.floor(Math.random() * 2_000_000_000),
    isDeleted: false,
    groupIds: [],
    frameId: null,
    boundElements: null,
    updated: Date.now(),
    link: null,
    locked: false,
  } as unknown as ExcalidrawElement;
}

function createText(options: TextOptions): ExcalidrawElement {
  const fontSize = options.fontSize ?? 16;
  return {
    id: options.id ?? makeId('text'),
    type: 'text',
    x: options.x,
    y: options.y,
    width: options.width ?? Math.max(100, options.text.length * 8),
    height: options.height ?? Math.round(fontSize * 1.5),
    angle: 0,
    strokeColor: options.strokeColor ?? '#212529',
    backgroundColor: 'transparent',
    fillStyle: 'solid',
    strokeWidth: 1,
    strokeStyle: 'solid',
    roundness: null,
    roughness: 1,
    opacity: 100,
    seed: Math.floor(Math.random() * 2_000_000_000),
    version: 1,
    versionNonce: Math.floor(Math.random() * 2_000_000_000),
    isDeleted: false,
    groupIds: [],
    frameId: null,
    boundElements: null,
    updated: Date.now(),
    link: null,
    locked: false,
    text: options.text,
    fontSize,
    fontFamily: 1,
    textAlign: options.textAlign ?? 'center',
    verticalAlign: 'middle',
    containerId: null,
    originalText: options.text,
    autoResize: true,
    lineHeight: 1.25,
  } as unknown as ExcalidrawElement;
}

function createArrow(startX: number, startY: number, endX: number, endY: number, strokeColor = '#6c757d'): ExcalidrawElement {
  const dx = endX - startX;
  const dy = endY - startY;
  return {
    id: makeId('arrow'),
    type: 'arrow',
    x: startX,
    y: startY,
    width: Math.max(1, Math.abs(dx)),
    height: Math.max(1, Math.abs(dy)),
    angle: 0,
    strokeColor,
    backgroundColor: 'transparent',
    fillStyle: 'solid',
    strokeWidth: 2,
    strokeStyle: 'solid',
    roundness: { type: 2 },
    roughness: 1,
    opacity: 100,
    seed: Math.floor(Math.random() * 2_000_000_000),
    version: 1,
    versionNonce: Math.floor(Math.random() * 2_000_000_000),
    isDeleted: false,
    groupIds: [],
    frameId: null,
    boundElements: null,
    updated: Date.now(),
    link: null,
    locked: false,
    points: [[0, 0], [dx, dy]],
    lastCommittedPoint: null,
    startBinding: null,
    endBinding: null,
    startArrowhead: null,
    endArrowhead: 'arrow',
    elbowed: false,
  } as unknown as ExcalidrawElement;
}

interface LabeledBoxOptions {
  type?: 'rectangle' | 'ellipse' | 'diamond';
  x: number;
  y: number;
  width: number;
  height: number;
  text: string;
  fontSize?: number;
  strokeColor?: string;
  backgroundColor?: string;
  textColor?: string;
  textAlign?: 'left' | 'center' | 'right';
  strokeStyle?: 'solid' | 'dashed';
}

function createLabeledBox(options: LabeledBoxOptions): ExcalidrawElement[] {
  const fontSize = options.fontSize ?? 14;
  const shape = createShape({
    type: options.type ?? 'rectangle',
    x: options.x,
    y: options.y,
    width: options.width,
    height: options.height,
    strokeColor: options.strokeColor,
    backgroundColor: options.backgroundColor,
    strokeStyle: options.strokeStyle,
  });
  const textElem = createText({
    text: options.text,
    x: options.x + 10,
    y: options.y + Math.max(8, Math.round((options.height - fontSize * 1.5) / 2)),
    width: options.width - 20,
    height: Math.round(fontSize * 1.5),
    fontSize,
    strokeColor: options.textColor ?? options.strokeColor ?? '#212529',
    textAlign: options.textAlign ?? 'center',
  });
  return [shape, textElem];
}

function normalizeKeyPoints(points?: readonly string[]): string[] {
  const filtered = points?.map((p) => p.trim()).filter((p) => p.length > 0) ?? [];
  return filtered.length > 0 ? filtered : [
    'Core Principle & Foundation',
    'Underlying Mechanism',
    'Empirical Evidence & Cases',
    'Practical Application & Impact',
  ];
}

function buildConceptMap(lessonTitle: string, rawPoints?: readonly string[]): ExcalidrawElement[] {
  const elements: ExcalidrawElement[] = [];
  const points = normalizeKeyPoints(rawPoints).slice(0, 6);
  const title = lessonTitle.trim() || 'Central Concept';

  elements.push(...createLabeledBox({ x: 100, y: 30, width: 760, height: 44, text: `Concept Map: ${title}`, fontSize: 18, backgroundColor: '#f8f9fa', strokeColor: '#adb5bd' }));

  const centerX = 480;
  const centerY = 320;
  const hubW = 240;
  const hubH = 90;

  elements.push(...createLabeledBox({ type: 'ellipse', x: centerX - hubW / 2, y: centerY - hubH / 2, width: hubW, height: hubH, text: title, fontSize: 18, strokeColor: '#1971c2', backgroundColor: '#e7f5ff', textColor: '#1864ab' }));

  const nodeW = 190;
  const nodeH = 70;
  const radiusX = 320;
  const radiusY = 190;
  const colors = [
    { bg: '#ebfbee', stroke: '#2b8a3e', text: '#237032' },
    { bg: '#fff4e6', stroke: '#d9480f', text: '#b83804' },
    { bg: '#f3f0ff', stroke: '#6741d9', text: '#502db3' },
    { bg: '#e3fafc', stroke: '#0c8599', text: '#09697a' },
    { bg: '#fff9db', stroke: '#e67700', text: '#b85a00' },
    { bg: '#f1f3f5', stroke: '#495057', text: '#343a40' },
  ];

  points.forEach((point, index) => {
    const angle = (2 * Math.PI * index) / points.length - Math.PI / 2;
    const nx = centerX + radiusX * Math.cos(angle);
    const ny = centerY + radiusY * Math.sin(angle);
    const color = colors[index % colors.length];

    elements.push(...createLabeledBox({ x: Math.round(nx - nodeW / 2), y: Math.round(ny - nodeH / 2), width: nodeW, height: nodeH, text: point, fontSize: 14, strokeColor: color.stroke, backgroundColor: color.bg, textColor: color.text }));

    const arrowStartX = centerX + (hubW / 2) * 0.8 * Math.cos(angle);
    const arrowStartY = centerY + (hubH / 2) * 0.8 * Math.sin(angle);
    const arrowEndX = nx - (nodeW / 2) * 0.8 * Math.cos(angle);
    const arrowEndY = ny - (nodeH / 2) * 0.8 * Math.sin(angle);
    elements.push(createArrow(Math.round(arrowStartX), Math.round(arrowStartY), Math.round(arrowEndX), Math.round(arrowEndY), color.stroke));
  });

  return elements;
}

function buildCompareContrast(lessonTitle: string, rawPoints?: readonly string[]): ExcalidrawElement[] {
  const elements: ExcalidrawElement[] = [];
  const points = normalizeKeyPoints(rawPoints);
  const title = lessonTitle.trim() || 'Comparison Matrix';
  const conceptA = points[0];
  const conceptB = points[1] ?? 'Concept B (Antithesis)';

  elements.push(...createLabeledBox({ x: 80, y: 30, width: 800, height: 48, text: `Compare & Contrast: ${title}`, fontSize: 18, backgroundColor: '#f1f3f5', strokeColor: '#495057' }));

  const columns = [
    { name: `Concept A: ${conceptA}`, x: 80, stroke: '#1971c2', bg: '#e7f5ff', text: '#1864ab', tag: 'A' },
    { name: `Concept B: ${conceptB}`, x: 500, stroke: '#d9480f', bg: '#fff4e6', text: '#c92a2a', tag: 'B' },
  ];

  columns.forEach((col) => {
    elements.push(...createLabeledBox({ x: col.x, y: 100, width: 380, height: 60, text: col.name, fontSize: 16, strokeColor: col.stroke, backgroundColor: col.bg, textColor: col.text }));
    const rows = [
      `${col.tag}1. Defining Attributes:\nCore assumptions, variables & scope`,
      `${col.tag}2. Primary Mechanism & Strengths:\nOptimal conditions & operational advantages`,
      `${col.tag}3. Limitations & Boundary Cases:\nBoundary constraints & where ${col.tag} breaks`,
    ];
    rows.forEach((rowText, rIdx) => {
      elements.push(...createLabeledBox({ x: col.x, y: 180 + rIdx * 100, width: 380, height: 80, text: rowText, fontSize: 13, strokeColor: '#ced4da', backgroundColor: '#ffffff', textColor: '#495057', textAlign: 'left' }));
    });
  });

  elements.push(...createLabeledBox({ x: 80, y: 490, width: 800, height: 75, text: 'Shared Core & Overlap: Common theoretical foundations and unified invariants', fontSize: 15, strokeColor: '#2b8a3e', backgroundColor: '#ebfbee', textColor: '#237032' }));
  elements.push(...createLabeledBox({ x: 80, y: 580, width: 800, height: 75, text: 'Key Distinctions & Decision Boundary: Criteria for choosing Concept A over Concept B', fontSize: 15, strokeColor: '#6741d9', backgroundColor: '#f3f0ff', textColor: '#502db3' }));

  return elements;
}

function buildProcessFlow(lessonTitle: string, rawPoints?: readonly string[]): ExcalidrawElement[] {
  const elements: ExcalidrawElement[] = [];
  const points = normalizeKeyPoints(rawPoints);
  const title = lessonTitle.trim() || 'Process Workflow';
  const steps = points.length >= 3 ? points.slice(0, 4) : [
    'Step 1: Input & Preconditions',
    'Step 2: Core Transformation',
    'Step 3: Verification & Invariants',
    'Step 4: Target Outcome',
  ];

  elements.push(...createLabeledBox({ x: 60, y: 30, width: 860, height: 48, text: `Process Flowchart: ${title}`, fontSize: 18, backgroundColor: '#f8f9fa', strokeColor: '#495057' }));

  const stepW = 180;
  const stepH = 80;
  const stepY = 110;
  const spacing = 44;
  const startX = 60;
  const stepColors = [
    { bg: '#e7f5ff', stroke: '#1971c2', text: '#1864ab' },
    { bg: '#f3f0ff', stroke: '#6741d9', text: '#502db3' },
    { bg: '#fff4e6', stroke: '#d9480f', text: '#b83804' },
    { bg: '#ebfbee', stroke: '#2b8a3e', text: '#237032' },
  ];

  steps.forEach((step, index) => {
    const sx = startX + index * (stepW + spacing);
    const color = stepColors[index % stepColors.length];
    elements.push(...createLabeledBox({ x: sx, y: stepY, width: stepW, height: stepH, text: step, fontSize: 14, strokeColor: color.stroke, backgroundColor: color.bg, textColor: color.text }));
    if (index < steps.length - 1) {
      elements.push(createArrow(sx + stepW, stepY + stepH / 2, sx + stepW + spacing, stepY + stepH / 2, '#868e96'));
    }
  });

  const cardH = 130;
  const cardY = 220;
  const annotations = [
    'Preconditions & Inputs:\n• Initial state & context\n• Raw inputs & variables\n• Foundational axioms',
    'Core Transformations:\n• Algorithmic operations\n• Mechanisms & formulas\n• Active state mutation',
    'Verification & Testing:\n• Integrity checks\n• Invariants & bounds\n• Edge case assertions',
    'Target Artifacts:\n• Final stable product\n• Long-term side effects\n• Downstream consumers',
  ];

  steps.forEach((_, index) => {
    const cx = startX + index * (stepW + spacing);
    elements.push(...createLabeledBox({ x: cx, y: cardY, width: stepW, height: cardH, text: annotations[index], fontSize: 12, strokeColor: '#adb5bd', backgroundColor: '#ffffff', textColor: '#495057', textAlign: 'left', strokeStyle: 'dashed' }));
  });

  elements.push(
    createArrow(startX + 3 * (stepW + spacing) + stepW / 2, cardY + cardH + 20, startX + stepW / 2, cardY + cardH + 20, '#fa5252'),
    createText({ text: 'Iterative Feedback & Continuous Verification Loop', x: startX + 160, y: cardY + cardH + 28, width: 500, fontSize: 13, strokeColor: '#e03131' }),
  );

  return elements;
}

function buildVisualAnalogy(lessonTitle: string, rawPoints?: readonly string[]): ExcalidrawElement[] {
  const elements: ExcalidrawElement[] = [];
  const points = normalizeKeyPoints(rawPoints);
  const title = lessonTitle.trim() || 'Dual Coding Synthesis';

  elements.push(
    ...createLabeledBox({ x: 60, y: 30, width: 860, height: 54, text: `Dual Coding Card: ${title}`, fontSize: 18, backgroundColor: '#f8f9fa', strokeColor: '#343a40' }),
    createText({ text: 'Verbal / Symbolic Representation (Left) ↔ Concrete / Spatial Mental Imagery (Right)', x: 80, y: 64, width: 820, fontSize: 12, strokeColor: '#868e96' }),
  );

  const colW = 410;
  const colH = 500;
  const colY = 105;

  elements.push(
    createShape({ type: 'rectangle', x: 60, y: colY, width: colW, height: colH, strokeColor: '#1971c2', backgroundColor: '#f8f9fa' }),
    ...createLabeledBox({ x: 60, y: colY, width: colW, height: 48, text: 'Verbal & Abstract Channel', fontSize: 16, strokeColor: '#1971c2', backgroundColor: '#e7f5ff', textColor: '#1864ab' }),
    ...createLabeledBox({ x: 80, y: colY + 68, width: colW - 40, height: 80, text: `1. Core Definitions & Formal Claims:\n${points[0]}`, fontSize: 13, strokeColor: '#ced4da', backgroundColor: '#ffffff', textColor: '#343a40', textAlign: 'left' }),
    ...createLabeledBox({ x: 80, y: colY + 164, width: colW - 40, height: 80, text: `2. Formulas, Proofs & Quantitative Logic:\n${points[1] ?? 'Mathematical relationships and equations.'}`, fontSize: 13, strokeColor: '#ced4da', backgroundColor: '#ffffff', textColor: '#343a40', textAlign: 'left' }),
    ...createLabeledBox({ x: 80, y: colY + 260, width: colW - 40, height: 80, text: `3. Boundary Conditions & Nuances:\n${points[2] ?? 'Edge cases, scope bounds, and qualifiers.'}`, fontSize: 13, strokeColor: '#ced4da', backgroundColor: '#ffffff', textColor: '#343a40', textAlign: 'left' }),
    ...createLabeledBox({ x: 80, y: colY + 356, width: colW - 40, height: 120, text: `4. Synthesis Note:\n${points[3] ?? 'Summary synthesizing the abstract logic into memory.'}`, fontSize: 13, strokeColor: '#ced4da', backgroundColor: '#ffffff', textColor: '#343a40', textAlign: 'left' }),
  );

  const rightX = 510;
  elements.push(
    createShape({ type: 'rectangle', x: rightX, y: colY, width: colW, height: colH, strokeColor: '#d9480f', backgroundColor: '#fdfbf7' }),
    ...createLabeledBox({ x: rightX, y: colY, width: colW, height: 48, text: 'Visual & Mental Imagery Channel', fontSize: 16, strokeColor: '#d9480f', backgroundColor: '#fff4e6', textColor: '#c92a2a' }),
    createShape({ type: 'rectangle', x: rightX + 20, y: colY + 68, width: colW - 40, height: 270, strokeColor: '#ffa8a8', backgroundColor: '#ffffff', strokeStyle: 'dashed' }),
    createText({ text: '[Spatial Diagram: Visual Mental Model]', x: rightX + 40, y: colY + 84, width: colW - 80, fontSize: 13, strokeColor: '#fa5252' }),
    ...createLabeledBox({ type: 'ellipse', x: rightX + 135, y: colY + 120, width: 140, height: 60, text: 'Core Concept', fontSize: 14, strokeColor: '#e03131', backgroundColor: '#ffe3e3', textColor: '#c92a2a' }),
    ...createLabeledBox({ type: 'diamond', x: rightX + 50, y: colY + 220, width: 90, height: 70, text: 'Factor A', fontSize: 13, strokeColor: '#1971c2', backgroundColor: '#e7f5ff', textColor: '#1864ab' }),
    ...createLabeledBox({ type: 'diamond', x: rightX + 270, y: colY + 220, width: 90, height: 70, text: 'Factor B', fontSize: 13, strokeColor: '#2b8a3e', backgroundColor: '#ebfbee', textColor: '#237032' }),
    createArrow(rightX + 160, colY + 180, rightX + 110, colY + 220, '#868e96'),
    createArrow(rightX + 250, colY + 180, rightX + 290, colY + 220, '#868e96'),
    ...createLabeledBox({ x: rightX + 20, y: colY + 356, width: colW - 40, height: 120, text: 'Dual Coding Bridge:\nPairing spatial layout and concrete geometry with the abstract formulas on the left grounds complex ideas in human visual memory.', fontSize: 13, strokeColor: '#ced4da', backgroundColor: '#ffffff', textColor: '#495057', textAlign: 'left' }),
    createArrow(470, colY + colH / 2, 510, colY + colH / 2, '#862e9c'),
  );

  return elements;
}

export function buildDualCodingSceneElements(
  templateId: DualCodingTemplateId,
  lessonTitle: string,
  keyPoints?: readonly string[],
): readonly ExcalidrawElement[] {
  switch (templateId) {
    case 'concept_map':
      return buildConceptMap(lessonTitle, keyPoints);
    case 'compare_contrast':
      return buildCompareContrast(lessonTitle, keyPoints);
    case 'process_flow':
      return buildProcessFlow(lessonTitle, keyPoints);
    case 'visual_analogy':
      return buildVisualAnalogy(lessonTitle, keyPoints);
    default: {
      const _exhaustiveCheck: never = templateId;
      throw new Error(`Unsupported dual coding template ID: ${_exhaustiveCheck}`);
    }
  }
}

export function buildLessonMermaidDiagram(lessonTitle: string, keyPoints?: readonly string[]): string {
  const sanitizedTitle = lessonTitle.replace(/["\n\r[\]()]/g, ' ').trim() || 'Lesson Overview';
  const points = normalizeKeyPoints(keyPoints).map((p) => p.replace(/["\n\r[\]()]/g, ' ').trim());

  const lines: string[] = [
    'flowchart TD',
    `    Core["${sanitizedTitle}"]`,
  ];

  points.forEach((point, index) => {
    const id = `Node_${index + 1}`;
    lines.push(`    ${id}["${point}"]`);
  });

  lines.push('');
  lines.push('    %% Concept linkages');
  points.forEach((_, index) => {
    const id = `Node_${index + 1}`;
    lines.push(`    Core --> ${id}`);
    if (index > 0) {
      const prevId = `Node_${index}`;
      lines.push(`    ${prevId} -.-> ${id}`);
    }
  });

  return lines.join('\n');
}
