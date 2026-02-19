import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';

import {
  LoadingState,
  InlineSpinner,
  SkeletonCard,
  SkeletonList,
  FullPageLoading,
  SectionLoading,
  ButtonLoading,
} from '../LoadingState';

describe('LoadingState', () => {
  describe('Rendering', () => {
    it('renders spinner by default', () => {
      render(<LoadingState />);

      const status = screen.getByRole('status');
      expect(status).toBeInTheDocument();
      expect(status).toHaveAttribute('aria-label', 'Loading');
    });

    it('renders spinner variant', () => {
      const { container } = render(<LoadingState type="spinner" />);

      const spinner = container.querySelector('.animate-spin');
      expect(spinner).toBeInTheDocument();
    });

    it('renders skeleton variant', () => {
      const { container } = render(<LoadingState type="skeleton" />);

      const skeleton = container.querySelector('.animate-pulse');
      expect(skeleton).toBeInTheDocument();
    });

    it('renders dots variant', () => {
      const { container } = render(<LoadingState type="dots" />);

      const dots = container.querySelectorAll('.animate-pulse');
      expect(dots).toHaveLength(3);
    });
  });

  describe('Sizes', () => {
    it('renders small spinner', () => {
      const { container } = render(<LoadingState type="spinner" size="sm" />);

      const spinner = container.querySelector('.w-4.h-4');
      expect(spinner).toBeInTheDocument();
    });

    it('renders medium spinner', () => {
      const { container } = render(<LoadingState type="spinner" size="md" />);

      const spinner = container.querySelector('.w-8.h-8');
      expect(spinner).toBeInTheDocument();
    });

    it('renders large spinner', () => {
      const { container } = render(<LoadingState type="spinner" size="lg" />);

      const spinner = container.querySelector('.w-12.h-12');
      expect(spinner).toBeInTheDocument();
    });
  });

  describe('Message Display', () => {
    it('displays custom message', () => {
      render(<LoadingState message="Loading documents..." />);

      expect(screen.getByText('Loading documents...', { selector: 'p' })).toBeInTheDocument();
    });

    it('sets aria-label from message', () => {
      render(<LoadingState message="Custom loading" />);

      const status = screen.getByRole('status');
      expect(status).toHaveAttribute('aria-label', 'Custom loading');
    });

    it('includes screen reader text', () => {
      render(<LoadingState message="Processing" />);

      const srText = screen.getByText('Processing', { selector: '.sr-only' });
      expect(srText).toBeInTheDocument();
    });

    it('uses default screen reader text when no message', () => {
      render(<LoadingState />);

      const srText = screen.getByText('Loading content', { selector: '.sr-only' });
      expect(srText).toBeInTheDocument();
    });
  });

  describe('Centering', () => {
    it('centers content by default', () => {
      const { container } = render(<LoadingState />);

      const wrapper = container.firstChild as HTMLElement;
      expect(wrapper).toHaveClass('flex', 'flex-col', 'items-center', 'justify-center');
    });

    it('aligns to start when centered is false', () => {
      const { container } = render(<LoadingState centered={false} />);

      const wrapper = container.firstChild as HTMLElement;
      expect(wrapper).toHaveClass('flex', 'flex-col', 'items-start');
      expect(wrapper).not.toHaveClass('justify-center');
    });
  });

  describe('Custom ClassName', () => {
    it('applies custom className', () => {
      const { container } = render(<LoadingState className="custom-class" />);

      const wrapper = container.firstChild as HTMLElement;
      expect(wrapper).toHaveClass('custom-class');
    });

    it('preserves default classes with custom className', () => {
      const { container } = render(<LoadingState className="my-custom" />);

      const wrapper = container.firstChild as HTMLElement;
      expect(wrapper).toHaveClass('my-custom', 'flex', 'flex-col');
    });
  });

  describe('Accessibility', () => {
    it('has role status', () => {
      render(<LoadingState />);

      expect(screen.getByRole('status')).toBeInTheDocument();
    });

    it('has aria-live polite', () => {
      render(<LoadingState />);

      const status = screen.getByRole('status');
      expect(status).toHaveAttribute('aria-live', 'polite');
    });

    it('has proper aria-label', () => {
      render(<LoadingState message="Test loading" />);

      const status = screen.getByRole('status');
      expect(status).toHaveAttribute('aria-label', 'Test loading');
    });

    it('hides visual elements from screen readers', () => {
      const { container } = render(<LoadingState type="spinner" />);

      const spinner = container.querySelector('.animate-spin');
      expect(spinner).toHaveAttribute('aria-hidden', 'true');
    });
  });
});

describe('InlineSpinner', () => {
  it('renders inline spinner', () => {
    const { container } = render(<InlineSpinner />);

    const spinner = container.querySelector('.animate-spin');
    expect(spinner).toBeInTheDocument();
    expect(spinner).toHaveClass('inline-block');
  });

  it('has proper size classes', () => {
    const { container } = render(<InlineSpinner />);

    const spinner = container.querySelector('.w-4.h-4');
    expect(spinner).toBeInTheDocument();
  });

  it('applies custom className', () => {
    const { container } = render(<InlineSpinner className="ml-2" />);

    const spinner = container.querySelector('.ml-2');
    expect(spinner).toBeInTheDocument();
  });

  it('has accessibility attributes', () => {
    render(<InlineSpinner />);

    const status = screen.getByRole('status');
    expect(status).toHaveAttribute('aria-label', 'Loading');
  });

  it('includes screen reader text', () => {
    render(<InlineSpinner />);

    const srText = screen.getByText('Loading', { selector: '.sr-only' });
    expect(srText).toBeInTheDocument();
  });
});

describe('SkeletonCard', () => {
  it('renders skeleton card', () => {
    const { container } = render(<SkeletonCard />);

    const card = container.querySelector('.animate-pulse');
    expect(card).toBeInTheDocument();
  });

  it('has proper styling', () => {
    const { container } = render(<SkeletonCard />);

    const card = container.querySelector('.rounded-lg.border');
    expect(card).toBeInTheDocument();
  });

  it('has title skeleton', () => {
    const { container } = render(<SkeletonCard />);

    const title = container.querySelector('.h-6');
    expect(title).toBeInTheDocument();
  });

  it('has content lines', () => {
    const { container } = render(<SkeletonCard />);

    const lines = container.querySelectorAll('.h-4');
    expect(lines.length).toBeGreaterThan(0);
  });

  it('has footer buttons', () => {
    const { container } = render(<SkeletonCard />);

    const buttons = container.querySelectorAll('.h-8');
    expect(buttons).toHaveLength(2);
  });

  it('is hidden from screen readers', () => {
    const { container } = render(<SkeletonCard />);

    const card = container.firstChild as HTMLElement;
    expect(card).toHaveAttribute('aria-hidden', 'true');
  });
});

describe('SkeletonList', () => {
  it('renders default 5 items', () => {
    const { container } = render(<SkeletonList />);

    const items = container.querySelectorAll('.animate-pulse');
    expect(items).toHaveLength(5);
  });

  it('renders custom count', () => {
    const { container } = render(<SkeletonList count={3} />);

    const items = container.querySelectorAll('.animate-pulse');
    expect(items).toHaveLength(3);
  });

  it('each item has icon placeholder', () => {
    const { container } = render(<SkeletonList count={2} />);

    const icons = container.querySelectorAll('.w-10.h-10');
    expect(icons).toHaveLength(2);
  });

  it('is hidden from screen readers', () => {
    const { container } = render(<SkeletonList />);

    const list = container.firstChild as HTMLElement;
    expect(list).toHaveAttribute('aria-hidden', 'true');
  });
});

describe('FullPageLoading', () => {
  it('renders full page loading', () => {
    render(<FullPageLoading />);

    const status = screen.getByRole('status');
    expect(status).toBeInTheDocument();
  });

  it('displays custom message', () => {
    render(<FullPageLoading message="Initializing app..." />);

    expect(screen.getByText('Initializing app...', { selector: 'p' })).toBeInTheDocument();
  });

  it('displays default message', () => {
    render(<FullPageLoading />);

    expect(screen.getByText('Loading...', { selector: 'p' })).toBeInTheDocument();
  });

  it('has full screen container', () => {
    const { container } = render(<FullPageLoading />);

    const wrapper = container.querySelector('.min-h-screen');
    expect(wrapper).toBeInTheDocument();
  });

  it('uses large spinner', () => {
    const { container } = render(<FullPageLoading />);

    const spinner = container.querySelector('.w-12.h-12');
    expect(spinner).toBeInTheDocument();
  });
});

describe('SectionLoading', () => {
  it('renders section loading', () => {
    render(<SectionLoading />);

    const status = screen.getByRole('status');
    expect(status).toBeInTheDocument();
  });

  it('displays custom message', () => {
    render(<SectionLoading message="Loading section..." />);

    expect(screen.getByText('Loading section...', { selector: 'p' })).toBeInTheDocument();
  });

  it('renders without overlay by default', () => {
    const { container } = render(<SectionLoading />);

    const overlay = container.querySelector('.absolute.inset-0');
    expect(overlay).not.toBeInTheDocument();
  });

  it('renders with overlay when specified', () => {
    const { container } = render(<SectionLoading overlay />);

    const overlay = container.querySelector('.absolute.inset-0');
    expect(overlay).toBeInTheDocument();
  });

  it('overlay has backdrop blur', () => {
    const { container } = render(<SectionLoading overlay />);

    const overlay = container.querySelector('.backdrop-blur-sm');
    expect(overlay).toBeInTheDocument();
  });

  it('overlay has proper z-index', () => {
    const { container } = render(<SectionLoading overlay />);

    const overlay = container.querySelector('.z-10');
    expect(overlay).toBeInTheDocument();
  });
});

describe('ButtonLoading', () => {
  it('renders button loading state', () => {
    render(<ButtonLoading />);

    const status = screen.getByRole('status');
    expect(status).toBeInTheDocument();
  });

  it('renders children alongside spinner', () => {
    render(<ButtonLoading>Save</ButtonLoading>);

    expect(screen.getByText('Save')).toBeInTheDocument();
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('has flex layout with gap', () => {
    const { container } = render(<ButtonLoading>Click</ButtonLoading>);

    const wrapper = container.querySelector('.flex.items-center.gap-2');
    expect(wrapper).toBeInTheDocument();
  });

  it('renders without children', () => {
    const { container } = render(<ButtonLoading />);

    const status = screen.getByRole('status');
    expect(status).toBeInTheDocument();
    expect(container.textContent).not.toContain('undefined');
  });
});

describe('Animation Performance', () => {
  it('spinner uses GPU-accelerated animation', () => {
    const { container } = render(<LoadingState type="spinner" />);

    const spinner = container.querySelector('.animate-spin');
    expect(spinner).toBeInTheDocument();
  });

  it('skeleton uses pulse animation', () => {
    const { container } = render(<LoadingState type="skeleton" />);

    const skeleton = container.querySelector('.animate-pulse');
    expect(skeleton).toBeInTheDocument();
  });

  it('dots have staggered animation delays', () => {
    const { container } = render(<LoadingState type="dots" />);

    const dots = container.querySelectorAll('.animate-pulse');
    expect(dots[0]).toHaveStyle({ animationDelay: '0ms' });
    expect(dots[1]).toHaveStyle({ animationDelay: '150ms' });
    expect(dots[2]).toHaveStyle({ animationDelay: '300ms' });
  });
});
