export type IconNode = readonly (readonly [tag: string, attributes: Record<string, string>])[];

/** Lucide's shapes, for the plain DOM that CodeMirror panels and gutters draw. */
export const icons = {
  search: [['circle', { cx: '11', cy: '11', r: '8' }], ['path', { d: 'm21 21-4.34-4.34' }]],
  caseSensitive: [
    ['path', { d: 'm2 16 4.039-9.69a.5.5 0 0 1 .923 0L11 16' }],
    ['path', { d: 'M22 9v7' }],
    ['path', { d: 'M3.304 13h6.392' }],
    ['circle', { cx: '18.5', cy: '12.5', r: '3.5' }],
  ],
  wholeWord: [
    ['circle', { cx: '7', cy: '12', r: '3' }],
    ['path', { d: 'M10 9v6' }],
    ['circle', { cx: '17', cy: '12', r: '3' }],
    ['path', { d: 'M14 7v8' }],
    ['path', { d: 'M22 17v1c0 .5-.5 1-1 1H3c-.5 0-1-.5-1-1v-1' }],
  ],
  regexp: [
    ['path', { d: 'M17 3v10' }],
    ['path', { d: 'm12.67 5.5 8.66 5' }],
    ['path', { d: 'm12.67 10.5 8.66-5' }],
    ['path', { d: 'M9 17a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v2a2 2 0 0 0 2 2h2a2 2 0 0 0 2-2v-2z' }],
  ],
  chevronUp: [['path', { d: 'm18 15-6-6-6 6' }]],
  chevronDown: [['path', { d: 'm6 9 6 6 6-6' }]],
  chevronRight: [['path', { d: 'm9 18 6-6-6-6' }]],
  close: [['path', { d: 'M18 6 6 18' }], ['path', { d: 'm6 6 12 12' }]],
} satisfies Record<string, IconNode>;

const SVG = 'http://www.w3.org/2000/svg';

export function icon(nodes: IconNode, size = 14, strokeWidth = 2): SVGSVGElement {
  const svg = document.createElementNS(SVG, 'svg');
  const attributes = {
    width: String(size), height: String(size), viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor',
    'stroke-width': String(strokeWidth), 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'aria-hidden': 'true',
  };
  for (const [name, value] of Object.entries(attributes)) svg.setAttribute(name, value);
  for (const [tag, shape] of nodes) {
    const element = document.createElementNS(SVG, tag);
    for (const [name, value] of Object.entries(shape)) element.setAttribute(name, value);
    svg.append(element);
  }
  return svg;
}
