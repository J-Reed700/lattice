import { render, screen } from '@testing-library/react';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

import { LearningCodeEditor } from './LearningCodeEditor';

let rangeGeometry: { getClientRects?: PropertyDescriptor; getBoundingClientRect?: PropertyDescriptor } = {};

beforeAll(() => {
  rangeGeometry = {
    getClientRects: Object.getOwnPropertyDescriptor(Range.prototype, 'getClientRects'),
    getBoundingClientRect: Object.getOwnPropertyDescriptor(Range.prototype, 'getBoundingClientRect'),
  };
  Object.defineProperty(Range.prototype, 'getClientRects', { configurable: true, value: () => [] });
  Object.defineProperty(Range.prototype, 'getBoundingClientRect', {
    configurable: true,
    value: () => new DOMRect(0, 0, 0, 16),
  });
});

afterAll(() => {
  if (rangeGeometry.getClientRects) Object.defineProperty(Range.prototype, 'getClientRects', rangeGeometry.getClientRects);
  else Reflect.deleteProperty(Range.prototype, 'getClientRects');
  if (rangeGeometry.getBoundingClientRect) Object.defineProperty(Range.prototype, 'getBoundingClientRect', rangeGeometry.getBoundingClientRect);
  else Reflect.deleteProperty(Range.prototype, 'getBoundingClientRect');
});

describe('LearningCodeEditor platform shortcuts', () => {
  const originalPlatform = navigator.platform;
  const originalUserAgent = navigator.userAgent;

  afterEach(() => {
    Object.defineProperty(navigator, 'platform', { value: originalPlatform, configurable: true });
    Object.defineProperty(navigator, 'userAgent', { value: originalUserAgent, configurable: true });
  });

  it('displays Ctrl+F shortcut hint on Windows', () => {
    Object.defineProperty(navigator, 'platform', { value: 'Win32', configurable: true });
    Object.defineProperty(navigator, 'userAgent', {
      value: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36',
      configurable: true,
    });

    render(<LearningCodeEditor path="main.py" value="print(1)" onChange={vi.fn()} />);

    const findButton = screen.getByRole('button', { name: /find in code/i });
    expect(findButton).toHaveAttribute('title', 'Find in code (Ctrl+F)');
    expect(screen.getByText('Ctrl+F')).toBeInTheDocument();
  });

  it('displays Ctrl+F shortcut hint on Linux', () => {
    Object.defineProperty(navigator, 'platform', { value: 'Linux x86_64', configurable: true });
    Object.defineProperty(navigator, 'userAgent', {
      value: 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36',
      configurable: true,
    });

    render(<LearningCodeEditor path="main.py" value="print(1)" onChange={vi.fn()} />);

    const findButton = screen.getByRole('button', { name: /find in code/i });
    expect(findButton).toHaveAttribute('title', 'Find in code (Ctrl+F)');
    expect(screen.getByText('Ctrl+F')).toBeInTheDocument();
  });

  it('displays ⌘F shortcut hint on Mac via navigator.userAgent when navigator.platform is empty', () => {
    Object.defineProperty(navigator, 'platform', { value: '', configurable: true });
    Object.defineProperty(navigator, 'userAgent', {
      value: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36',
      configurable: true,
    });

    render(<LearningCodeEditor path="main.py" value="print(1)" onChange={vi.fn()} />);

    const findButton = screen.getByRole('button', { name: /find in code/i });
    expect(findButton).toHaveAttribute('title', 'Find in code (⌘F)');
    expect(screen.getByText('⌘F')).toBeInTheDocument();
  });

  it('displays the run keyboard shortcut hint when onRun prop is provided on Mac', () => {
    Object.defineProperty(navigator, 'platform', { value: '', configurable: true });
    Object.defineProperty(navigator, 'userAgent', {
      value: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36',
      configurable: true,
    });

    render(<LearningCodeEditor path="main.py" value="print(1)" onChange={vi.fn()} onRun={vi.fn()} />);

    expect(screen.getByText(/⌘Enter run/)).toBeInTheDocument();
  });
});
