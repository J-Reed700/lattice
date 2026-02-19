/**
 * Toast System Tests
 *
 * Tests for toast notification system components and functionality
 */

import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

import { useToast } from '../../../hooks/useToast';
import { toastStore, toast} from '../../../stores/toastStore';
import { ToastContainer } from '../ToastContainer';
import { ToastItem } from '../ToastItem';

describe('Toast Store', () => {
  beforeEach(() => {
    toastStore.dismissAll();
  });

  it('should add a toast', () => {
    const id = toast.success('Test message');
    const toasts = toastStore.getToasts();

    expect(toasts).toHaveLength(1);
    expect(toasts[0].title).toBe('Test message');
    expect(toasts[0].type).toBe('success');
    expect(toasts[0].id).toBe(id);
  });

  it('should add multiple toasts', () => {
    toast.success('First');
    toast.error('Second');
    toast.warning('Third');

    const toasts = toastStore.getToasts();
    expect(toasts).toHaveLength(3);
  });

  it('should dismiss a toast', () => {
    const id = toast.info('Test');
    expect(toastStore.getToasts()).toHaveLength(1);

    toast.dismiss(id);
    expect(toastStore.getToasts()).toHaveLength(0);
  });

  it('should dismiss all toasts', () => {
    toast.success('One');
    toast.success('Two');
    toast.success('Three');

    expect(toastStore.getToasts()).toHaveLength(3);

    toast.dismissAll();
    expect(toastStore.getToasts()).toHaveLength(0);
  });

  it('should respect max toasts limit', () => {
    toastStore.updateConfig({ maxToasts: 3 });

    toast.info('1');
    toast.info('2');
    toast.info('3');
    toast.info('4'); // Should remove oldest

    const toasts = toastStore.getToasts();
    expect(toasts).toHaveLength(3);
    expect(toasts[0].title).toBe('4');
    expect(toasts[2].title).toBe('2');
  });

  it('should update configuration', () => {
    toastStore.updateConfig({
      position: 'bottom-left',
      maxToasts: 10,
      defaultDuration: 6000,
    });

    const config = toastStore.getConfig();
    expect(config.position).toBe('bottom-left');
    expect(config.maxToasts).toBe(10);
    expect(config.defaultDuration).toBe(6000);
  });

  it('should notify subscribers', () => {
    const listener = vi.fn();
    const unsubscribe = toastStore.subscribe(listener);

    toast.success('Test');
    expect(listener).toHaveBeenCalledTimes(1);

    toast.dismiss(toastStore.getToasts()[0].id);
    expect(listener).toHaveBeenCalledTimes(2);

    unsubscribe();
    toast.success('Test 2');
    expect(listener).toHaveBeenCalledTimes(2); // Not called after unsubscribe
  });
});

describe('Toast Types', () => {
  beforeEach(() => {
    toastStore.dismissAll();
  });

  it('should create success toast', () => {
    toast.success('Success message');
    const toasts = toastStore.getToasts();

    expect(toasts[0].type).toBe('success');
    expect(toasts[0].title).toBe('Success message');
  });

  it('should create error toast', () => {
    toast.error('Error message', { message: 'Details' });
    const toasts = toastStore.getToasts();

    expect(toasts[0].type).toBe('error');
    expect(toasts[0].title).toBe('Error message');
    expect(toasts[0].message).toBe('Details');
  });

  it('should create warning toast', () => {
    toast.warning('Warning message');
    const toasts = toastStore.getToasts();

    expect(toasts[0].type).toBe('warning');
  });

  it('should create info toast', () => {
    toast.info('Info message');
    const toasts = toastStore.getToasts();

    expect(toasts[0].type).toBe('info');
  });
});

describe('Toast Options', () => {
  beforeEach(() => {
    toastStore.dismissAll();
  });

  it('should set custom duration', () => {
    toast.success('Test', { duration: 10000 });
    const toasts = toastStore.getToasts();

    expect(toasts[0].duration).toBe(10000);
  });

  it('should set action button', () => {
    const onClick = vi.fn();
    toast.success('Test', {
      action: {
        label: 'Click me',
        onClick,
      },
    });

    const toasts = toastStore.getToasts();
    expect(toasts[0].action).toBeDefined();
    expect(toasts[0].action?.label).toBe('Click me');

    toasts[0].action?.onClick();
    expect(onClick).toHaveBeenCalled();
  });

  it('should set dismissible option', () => {
    toast.success('Test', { dismissible: false });
    const toasts = toastStore.getToasts();

    expect(toasts[0].dismissible).toBe(false);
  });

  it('should use default duration if not specified', () => {
    toastStore.updateConfig({ defaultDuration: 5000 });
    toast.success('Test');

    const toasts = toastStore.getToasts();
    expect(toasts[0].duration).toBe(5000);
  });
});

describe('ToastContainer', () => {
  beforeEach(() => {
    toastStore.dismissAll();
  });

  it('should render toast container', () => {
    toast.success('Test message');
    render(<ToastContainer />);

    expect(screen.getByText('Test message')).toBeInTheDocument();
  });

  it('should render multiple toasts', () => {
    toast.success('First');
    toast.error('Second');
    toast.info('Third');

    render(<ToastContainer />);

    expect(screen.getByText('First')).toBeInTheDocument();
    expect(screen.getByText('Second')).toBeInTheDocument();
    expect(screen.getByText('Third')).toBeInTheDocument();
  });

  it('should not render when no toasts', () => {
    const { container } = render(<ToastContainer />);
    expect(container.firstChild).toBeNull();
  });

  it('should dismiss all toasts on Escape key', () => {
    toast.success('Test');
    render(<ToastContainer />);

    expect(screen.getByText('Test')).toBeInTheDocument();

    fireEvent.keyDown(window, { key: 'Escape' });

    waitFor(() => {
      expect(screen.queryByText('Test')).not.toBeInTheDocument();
    });
  });
});

describe('ToastItem', () => {
  const mockToast = {
    id: '1',
    type: 'success' as const,
    title: 'Test Toast',
    message: 'Test message',
    duration: 4000,
    dismissible: true,
    createdAt: Date.now(),
  };

  it('should render toast with title', () => {
    render(
      <ToastItem
        toast={mockToast}
        onDismiss={vi.fn()}
        pauseOnHover
        index={0}
      />
    );

    expect(screen.getByText('Test Toast')).toBeInTheDocument();
  });

  it('should render toast with message', () => {
    render(
      <ToastItem
        toast={mockToast}
        onDismiss={vi.fn()}
        pauseOnHover
        index={0}
      />
    );

    expect(screen.getByText('Test message')).toBeInTheDocument();
  });

  it('should call onDismiss when close button clicked', () => {
    const onDismiss = vi.fn();
    render(
      <ToastItem
        toast={mockToast}
        onDismiss={onDismiss}
        pauseOnHover
        index={0}
      />
    );

    const closeButton = screen.getByLabelText('Dismiss notification');
    fireEvent.click(closeButton);

    waitFor(() => {
      expect(onDismiss).toHaveBeenCalledWith('1');
    });
  });

  it('should render action button', () => {
    const onClick = vi.fn();
    const toastWithAction = {
      ...mockToast,
      action: {
        label: 'Undo',
        onClick,
      },
    };

    render(
      <ToastItem
        toast={toastWithAction}
        onDismiss={vi.fn()}
        pauseOnHover
        index={0}
      />
    );

    const actionButton = screen.getByText('Undo');
    expect(actionButton).toBeInTheDocument();

    fireEvent.click(actionButton);
    expect(onClick).toHaveBeenCalled();
  });

  it('should not render close button when not dismissible', () => {
    const nonDismissibleToast = {
      ...mockToast,
      dismissible: false,
    };

    render(
      <ToastItem
        toast={nonDismissibleToast}
        onDismiss={vi.fn()}
        pauseOnHover
        index={0}
      />
    );

    expect(screen.queryByLabelText('Dismiss notification')).not.toBeInTheDocument();
  });

  it('should render progress bar when duration is set', () => {
    render(
      <ToastItem
        toast={mockToast}
        onDismiss={vi.fn()}
        pauseOnHover
        index={0}
      />
    );

    const progressBar = screen.getByRole('progressbar');
    expect(progressBar).toBeInTheDocument();
  });
});

describe('useToast Hook', () => {
  beforeEach(() => {
    toastStore.dismissAll();
  });

  it('should return toast functions', () => {
    let result: ReturnType<typeof useToast>;

    function TestComponent() {
      result = useToast();
      return null;
    }

    render(<TestComponent />);

    expect(result!.toast.success).toBeDefined();
    expect(result!.toast.error).toBeDefined();
    expect(result!.toast.warning).toBeDefined();
    expect(result!.toast.info).toBeDefined();
    expect(result!.toast.dismiss).toBeDefined();
    expect(result!.toast.dismissAll).toBeDefined();
  });

  it('should create toasts via hook', () => {
    function TestComponent() {
      const { toast } = useToast();

      return (
        <button onClick={() => toast.success('Test')}>
          Show Toast
        </button>
      );
    }

    render(<TestComponent />);

    const button = screen.getByText('Show Toast');
    fireEvent.click(button);

    expect(toastStore.getToasts()).toHaveLength(1);
    expect(toastStore.getToasts()[0].title).toBe('Test');
  });
});

describe('Accessibility', () => {
  beforeEach(() => {
    toastStore.dismissAll();
  });

  it('should have ARIA role for container', () => {
    toast.success('Test');
    render(<ToastContainer />);

    const container = screen.getByRole('region');
    expect(container).toHaveAttribute('aria-label', 'Notifications');
  });

  it('should have ARIA live region', () => {
    toast.success('Test');
    render(<ToastContainer />);

    const container = screen.getByRole('region');
    expect(container).toHaveAttribute('aria-live', 'polite');
  });

  it('should have ARIA role for toast item', () => {
    const mockToast = {
      id: '1',
      type: 'success' as const,
      title: 'Test',
      duration: 4000,
      dismissible: true,
      createdAt: Date.now(),
    };

    render(
      <ToastItem
        toast={mockToast}
        onDismiss={vi.fn()}
        pauseOnHover
        index={0}
      />
    );

    const toast = screen.getByRole('status');
    expect(toast).toBeInTheDocument();
  });

  it('should have ARIA label for close button', () => {
    const mockToast = {
      id: '1',
      type: 'success' as const,
      title: 'Test',
      duration: 4000,
      dismissible: true,
      createdAt: Date.now(),
    };

    render(
      <ToastItem
        toast={mockToast}
        onDismiss={vi.fn()}
        pauseOnHover
        index={0}
      />
    );

    const closeButton = screen.getByLabelText('Dismiss notification');
    expect(closeButton).toBeInTheDocument();
  });

  it('should have progress bar with ARIA attributes', () => {
    const mockToast = {
      id: '1',
      type: 'info' as const,
      title: 'Test',
      duration: 4000,
      dismissible: true,
      createdAt: Date.now(),
    };

    render(
      <ToastItem
        toast={mockToast}
        onDismiss={vi.fn()}
        pauseOnHover
        index={0}
      />
    );

    const progressBar = screen.getByRole('progressbar');
    expect(progressBar).toHaveAttribute('aria-label', 'Time remaining');
    expect(progressBar).toHaveAttribute('aria-valuemin', '0');
    expect(progressBar).toHaveAttribute('aria-valuemax', '100');
  });
});

describe('Auto-dismiss', () => {
  beforeEach(() => {
    toastStore.dismissAll();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should auto-dismiss after duration', () => {
    toast.success('Test', { duration: 3000 });
    expect(toastStore.getToasts()).toHaveLength(1);

    vi.advanceTimersByTime(3000);

    expect(toastStore.getToasts()).toHaveLength(0);
  });

  it('should not auto-dismiss when duration is 0', () => {
    toast.info('Test', { duration: 0 });
    expect(toastStore.getToasts()).toHaveLength(1);

    vi.advanceTimersByTime(10000);

    expect(toastStore.getToasts()).toHaveLength(1);
  });
});
