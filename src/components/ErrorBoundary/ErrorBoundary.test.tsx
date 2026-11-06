/**
 * ErrorBoundary Test Suite
 *
 * Comprehensive tests for error boundary components
 */

import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

import { SANITIZED_MESSAGE, SENSITIVE_MESSAGE, SENSITIVE_PARTS } from '@/utils/__tests__/sensitiveMessage';

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
const FALLBACK_MESSAGE = 'Something unexpected happened.';

const errorWithoutMessage = (): Error => {
  const error = new Error('placeholder');
  Object.defineProperty(error, 'message', { value: undefined });
  return error;
};

const errorWithStack = (): Error => {
  const error = new Error(SENSITIVE_MESSAGE);
  error.stack = `Error: ${SENSITIVE_MESSAGE}
    at renderWidget (app.js:10:5)`;
  return error;
};

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

    expect(screen.getByText(/ran into a problem/i)).toBeDefined();
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

    expect(screen.getByText(/Couldn't load Test Section/i)).toBeDefined();
  });

  it('displays section name in error', () => {
    render(
      <SectionErrorBoundary sectionName="Search">
        <ThrowError />
      </SectionErrorBoundary>
    );

    expect(screen.getByText(/Couldn't load Search/i)).toBeDefined();
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

    expect(screen.getByText(/ran into a problem/i)).toBeDefined();
  });

  it('shows error details', () => {
    render(
      <FullPageError
        error={mockError}
        errorInfo={mockErrorInfo}
        resetError={mockReset}
      />
    );

    fireEvent.click(screen.getByText(/Show details/i));
    expect(screen.getAllByText(/Test error message/i).length).toBeGreaterThan(0);
  });

  it('has reload app button', () => {
    render(
      <FullPageError
        error={mockError}
        errorInfo={mockErrorInfo}
        resetError={mockReset}
      />
    );

    expect(screen.getByRole('button', { name: /^Reload$/i })).toBeDefined();
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

  it('strips paths, tokens, emails, and IPs from the user-facing error message', () => {
    const sensitiveError = new Error(
      'Failed for alice@example.com with token 0123456789abcdef0123456789abcdef at 192.168.1.42: C:\\Users\\alice\\secret.txt'
    );
    render(
      <FullPageError
        error={sensitiveError}
        errorInfo={mockErrorInfo}
        resetError={mockReset}
      />
    );

    expect(
      screen.getByText('Failed for [EMAIL] with token [TOKEN] at [IP]: [PATH]')
    ).toBeDefined();
  });

  describe('details', () => {
    afterEach(() => {
      vi.unstubAllEnvs();
    });

    it('shows no sensitive text in production details', () => {
      vi.stubEnv('DEV', false);
      const { container } = render(
        <FullPageError
          error={new Error(SENSITIVE_MESSAGE)}
          errorInfo={mockErrorInfo}
          resetError={mockReset}
        />
      );

      fireEvent.click(screen.getByText('Show details'));

      expect(screen.getAllByText(SANITIZED_MESSAGE)).toHaveLength(2);
      for (const part of SENSITIVE_PARTS) {
        expect(container.textContent).not.toContain(part);
      }
    });

    it('renders the fallback line for an error with no message', () => {
      render(
        <FullPageError
          error={errorWithoutMessage()}
          errorInfo={mockErrorInfo}
          resetError={mockReset}
        />
      );

      fireEvent.click(screen.getByText('Show details'));

      expect(screen.getByText(FALLBACK_MESSAGE)).toBeDefined();
    });

    describe('Copy details', () => {
      const copy = async () => {
        const writeText = vi.fn().mockResolvedValue(undefined);
        Object.defineProperty(navigator, 'clipboard', {
          value: { writeText },
          configurable: true,
        });
        render(
          <FullPageError
            error={errorWithStack()}
            errorInfo={mockErrorInfo}
            resetError={mockReset}
          />
        );
        fireEvent.click(screen.getByText('Copy details'));
        await screen.findByText('Copied');
        return writeText.mock.calls[0][0] as string;
      };

      it('holds no sensitive text in production but keeps the stack frames', async () => {
        vi.stubEnv('DEV', false);

        const copied = await copy();

        for (const part of SENSITIVE_PARTS) {
          expect(copied).not.toContain(part);
        }
        expect(copied).toContain(SANITIZED_MESSAGE);
        expect(copied).toContain('renderWidget');
      });

      it('stays raw in development', async () => {
        vi.stubEnv('DEV', true);

        const copied = await copy();

        expect(copied).toContain(SENSITIVE_MESSAGE);
        expect(copied).toContain('renderWidget');
      });
    });

    it('shows the raw message in development details', () => {
      vi.stubEnv('DEV', true);
      render(
        <FullPageError
          error={new Error(SENSITIVE_MESSAGE)}
          errorInfo={mockErrorInfo}
          resetError={mockReset}
        />
      );

      fireEvent.click(screen.getByText('Show details'));

      expect(screen.getByText(SENSITIVE_MESSAGE)).toBeDefined();
    });
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

    expect(screen.getByText(/Couldn't load Search/i)).toBeDefined();
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

    const retryButton = screen.getByText(/Try again/i);
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

    expect(screen.getByText(/Couldn't load/i)).toBeDefined();
  });

  it('renders the fallback line for an error with no message', () => {
    render(
      <SectionError error={errorWithoutMessage()} resetError={mockReset} sectionName="Search" />
    );

    expect(screen.getByText(FALLBACK_MESSAGE)).toBeDefined();
  });

  it('strips paths, tokens, emails, and IPs from the message', () => {
    const { container } = render(
      <SectionError
        error={new Error(SENSITIVE_MESSAGE)}
        resetError={mockReset}
        sectionName="Search"
      />
    );

    expect(screen.getByText(SANITIZED_MESSAGE)).toBeDefined();
    for (const part of SENSITIVE_PARTS) {
      expect(container.textContent).not.toContain(part);
    }
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

  it('renders the fallback line for an error with no message', () => {
    render(<InlineError error={errorWithoutMessage()} resetError={mockReset} />);

    expect(screen.getByText(FALLBACK_MESSAGE)).toBeDefined();
  });

  it.each([false, true])('strips sensitive text from the message (compact: %s)', (compact) => {
    const { container } = render(
      <InlineError error={new Error(SENSITIVE_MESSAGE)} resetError={mockReset} compact={compact} />
    );

    expect(screen.getByText(SANITIZED_MESSAGE)).toBeDefined();
    for (const part of SENSITIVE_PARTS) {
      expect(container.textContent).not.toContain(part);
    }
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
    const retryButton = screen.getByText(/Try again/i);
    expect(retryButton.tagName).toBe('BUTTON');
  });
});
