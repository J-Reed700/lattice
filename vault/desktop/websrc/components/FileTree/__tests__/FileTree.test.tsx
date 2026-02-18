import { render } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';

import { FileTree } from '../FileTree';

vi.mock('../../FileBrowser', () => ({
  FileBrowser: () => <div data-testid="file-browser">FileBrowser Component</div>,
}));

describe('FileTree', () => {
  describe('Rendering', () => {
    it('renders FileBrowser component', () => {
      const { getByTestId } = render(<FileTree />);
      expect(getByTestId('file-browser')).toBeInTheDocument();
    });

    it('applies correct container classes', () => {
      const { container } = render(<FileTree />);
      const wrapper = container.firstChild as HTMLElement;

      expect(wrapper).toHaveClass('flex-1', 'flex', 'flex-col', 'h-full');
    });
  });

  describe('Layout', () => {
    it('takes full height', () => {
      const { container } = render(<FileTree />);
      const wrapper = container.firstChild as HTMLElement;

      expect(wrapper).toHaveClass('h-full');
    });

    it('uses flex layout', () => {
      const { container } = render(<FileTree />);
      const wrapper = container.firstChild as HTMLElement;

      expect(wrapper).toHaveClass('flex', 'flex-col');
    });

    it('is flexible in parent container', () => {
      const { container } = render(<FileTree />);
      const wrapper = container.firstChild as HTMLElement;

      expect(wrapper).toHaveClass('flex-1');
    });
  });
});
