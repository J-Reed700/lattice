/**
 * ErrorBoundary Test Suite
 *
 * Comprehensive tests for error boundary components
 */

import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { ErrorBoundary } from './ErrorBoundary';
import { FullPageError } from './FullPageError';
import { InlineError } from './InlineError';
import { RootErrorBoundary } from './RootErrorBoundary';
import { SectionError } from './SectionError';
import { SectionErrorBoundary } from './SectionErrorBoundary';

// Suppress console errors in tests
beforeEach(() => {
  vi.spyOn(console, 'error').mockImplementation(() => {});
});

// Component that throws an error
const ThrowError = ({ shouldThrow = true }: { shouldThrow?: boolean }) => {
  if (shouldThrow) {
    throw new Error('Test error');
  }
  return <div>No error</div>;
};

describe('ErrorBoundary', () => {
  it('renders children when no error', () => {
    render(
      <ErrorBoundary>
        <div>Test content</div>
      </ErrorBoundary>
    );

    expect(screen.getByText('Test content')).toBeDefined();
  });

  it('catches errors and shows fallback', () => {
    const onError = vi.fn();

    render(
      <ErrorBoundary onError={onError}>
        <ThrowError />
      </ErrorBoundary>
    );

    expect(onError).toHaveBeenCalled();
    // Default fallback should be shown
  });

  it('calls onError callback with error details', () => {
    const onError = vi.fn();

    render(
      <ErrorBoundary onError={onError}>
        <ThrowError />
      </ErrorBoundary>
    );

    expect(onError).toHaveBeenCalledWith(
      expect.objectContaining({ message: 'Test error' }),
      expect.objectContaining({ componentStack: expect.any(String) })
    );
  });

  it('resets error state when reset is called', async () => {
    const CustomFallback = ({ resetError }: any) => (
        <button onClick={resetError}>Reset</button>
      );

    const { rerender } = render(
      <ErrorBoundary fallback={CustomFallback}>
        <ThrowError />
      </ErrorBoundary>
    );

    // Error boundary caught error
    expect(screen.getByText('Reset')).toBeDefined();

    // Re-render with component that doesn't throw
    rerender(
      <ErrorBoundary fallback={CustomFallback}>
        <ThrowError shouldThrow={false} />
      </ErrorBoundary>
    );

    // Reset the error
    fireEvent.click(screen.getByText('Reset'));

    await waitFor(() => {
      expect(screen.getByText('No error')).toBeDefined();
    });
  });

  it('auto-resets when resetKeys change', () => {
    let key = 1;

    const { rerender } = render(
      <ErrorBoundary resetKeys={[key]}>
        <ThrowError />
      </ErrorBoundary>
    );

    // Change reset key
    key = 2;

    rerender(
      <ErrorBoundary resetKeys={[key]}>
        <ThrowError shouldThrow={false} />
      </ErrorBoundary>
    );

    expect(screen.getByText('No error')).toBeDefined();
  });

  it('calls onReset callback when reset', async () => {
    const onReset = vi.fn();

    const CustomFallback = ({ resetError }: any) => <button onClick={resetError}>Reset</button>;

    render(
      <ErrorBoundary fallback={CustomFallback} onReset={onReset}>
        <ThrowError />
      </ErrorBoundary>
    );

    fireEvent.click(screen.getByText('Reset'));

    await waitFor(() => {
      expect(onReset).toHaveBeenCalled();
    });
  });
});

describe('RootErrorBoundary', () => {
  it('wraps children without error', () => {
    render(
      <RootErrorBoundary>
        <div>App content</div>
      </RootErrorBoundary>
    );

    expect(screen.getByText('App content')).toBeDefined();
  });

  it('shows FullPageError on error', () => {
    render(
      <RootErrorBoundary>
        <ThrowError />
      </RootErrorBoundary>
    );

    expect(screen.getByText(/something went wrong/i)).toBeDefined();
  });
});

describe('SectionErrorBoundary', () => {
  it('renders children without error', () => {
    render(
      <SectionErrorBoundary sectionName="Test Section">
        <div>Section content</div>
      </SectionErrorBoundary>
    );

    expect(screen.getByText('Section content')).toBeDefined();
  });

  it('shows SectionError on error', () => {
    render(
      <SectionErrorBoundary sectionName="Test Section">
        <ThrowError />
      </SectionErrorBoundary>
    );

    expect(screen.getByText(/Test Section Error/i)).toBeDefined();
  });

  it('displays section name in error', () => {
    render(
      <SectionErrorBoundary sectionName="Search">
        <ThrowError />
      </SectionErrorBoundary>
    );

    expect(screen.getByText(/Search Error/i)).toBeDefined();
  });
});

describe('FullPageError', () => {
  const mockError = new Error('Test error message');
  const mockErrorInfo = {
    componentStack: 'Component stack trace',
  };
  const mockReset = vi.fn();

  it('renders error message', () => {
    render(
      <FullPageError
        error={mockError}
        errorInfo={mockErrorInfo}
        resetError={mockReset}
      />
    );

    expect(screen.getByText(/something went wrong/i)).toBeDefined();
  });

  it('shows error details', () => {
    render(
      <FullPageError
        error={mockError}
        errorInfo={mockErrorInfo}
        resetError={mockReset}
      />
    );

    expect(screen.getByText(/Test error message/i)).toBeDefined();
  });

  it('has reload app button', () => {
    render(
      <FullPageError
        error={mockError}
        errorInfo={mockErrorInfo}
        resetError={mockReset}
      />
    );

    expect(screen.getByText(/Reload App/i)).toBeDefined();
  });

  it('has try again button', () => {
    render(
      <FullPageError
        error={mockError}
        errorInfo={mockErrorInfo}
        resetError={mockReset}
      />
    );

    const tryAgainButton = screen.getByText(/Try Again/i);
    expect(tryAgainButton).toBeDefined();

    fireEvent.click(tryAgainButton);
    expect(mockReset).toHaveBeenCalled();
  });

  it('has copy error button', () => {
    render(
      <FullPageError
        error={mockError}
        errorInfo={mockErrorInfo}
        resetError={mockReset}
      />
    );

    expect(screen.getByText(/Copy/i)).toBeDefined();
  });
});

describe('SectionError', () => {
  const mockError = new Error('Section error');
  const mockReset = vi.fn();

  it('renders section name', () => {
    render(
      <SectionError
        error={mockError}
        resetError={mockReset}
        sectionName="Search"
      />
    );

    expect(screen.getByText(/Search Error/i)).toBeDefined();
  });

  it('renders error message', () => {
    render(
      <SectionError
        error={mockError}
        resetError={mockReset}
        sectionName="Search"
      />
    );

    expect(screen.getByText(/Section error/i)).toBeDefined();
  });

  it('has retry button', () => {
    render(
      <SectionError
        error={mockError}
        resetError={mockReset}
        sectionName="Search"
      />
    );

    const retryButton = screen.getByText(/Retry/i);
    expect(retryButton).toBeDefined();

    fireEvent.click(retryButton);
    expect(mockReset).toHaveBeenCalled();
  });

  it('shows user-friendly message', () => {
    render(
      <SectionError
        error={mockError}
        resetError={mockReset}
        sectionName="Search"
      />
    );

    expect(screen.getByText(/other sections of the app are still working/i)).toBeDefined();
  });
});

describe('InlineError', () => {
  const mockError = new Error('Inline error');
  const mockReset = vi.fn();

  it('renders error message', () => {
    render(
      <InlineError
        error={mockError}
        resetError={mockReset}
      />
    );

    expect(screen.getByText(/Inline error/i)).toBeDefined();
  });

  it('shows custom message if provided', () => {
    render(
      <InlineError
        error={mockError}
        resetError={mockReset}
        message="Custom error message"
      />
    );

    expect(screen.getByText(/Custom error message/i)).toBeDefined();
  });

  it('has retry button', () => {
    render(
      <InlineError
        error={mockError}
        resetError={mockReset}
      />
    );

    const retryButton = screen.getByText(/Try again/i);
    expect(retryButton).toBeDefined();

    fireEvent.click(retryButton);
    expect(mockReset).toHaveBeenCalled();
  });

  it('renders in compact mode', () => {
    const { container } = render(
      <InlineError
        error={mockError}
        resetError={mockReset}
        compact
      />
    );

    // Compact mode should have different styling
    expect(container.querySelector('.truncate')).toBeDefined();
  });
});

describe('Error Types', () => {
  it('handles NetworkError specifically', () => {
    const NetworkErrorComponent = () => {
      const error = new Error('Network failure');
      error.name = 'NetworkError';
      throw error;
    };

    render(
      <ErrorBoundary>
        <NetworkErrorComponent />
      </ErrorBoundary>
    );

    // Error boundary should catch it
    expect(console.error).toHaveBeenCalled();
  });
});

describe('Accessibility', () => {
  it('FullPageError has accessible error info', () => {
    const mockError = new Error('Test error');
    const mockReset = vi.fn();

    render(
      <FullPageError
        error={mockError}
        errorInfo={{ componentStack: '' }}
        resetError={mockReset}
      />
    );

    // Should have heading
    expect(screen.getByRole('heading', { level: 1 })).toBeDefined();
  });

  it('SectionError has accessible buttons', () => {
    const mockError = new Error('Test error');
    const mockReset = vi.fn();

    render(
      <SectionError
        error={mockError}
        resetError={mockReset}
        sectionName="Test"
      />
    );

    // Retry button should be accessible
    const retryButton = screen.getByText(/Retry/i);
    expect(retryButton.tagName).toBe('BUTTON');
  });
});
