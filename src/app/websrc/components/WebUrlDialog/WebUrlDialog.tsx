/**
 * WebUrlDialog Component
 *
 * Purpose: Modal dialog for quick web URL ingestion from anywhere in the app
 *
 * Features:
 * - Full-screen modal with backdrop
 * - WebUrlInput form inside
 * - ESC key to close
 * - Backdrop click to close
 * - Automatically closes on successful ingestion
 *
 * Use cases:
 * - Quick capture from keyboard shortcut
 * - Menu item trigger
 * - Context menu action
 *
 * States: open, closed
 * Accessibility: Focus trap, ARIA dialog, keyboard navigation
 */

import { type WebIngestResponse } from '../../types';
import Dialog from '../ui/Dialog/Dialog';
import { WebUrlInput } from '../WebUrlInput';

interface WebUrlDialogProps {
  /** Dialog open state */
  open: boolean;
  /** Callback when dialog should close */
  onOpenChange: (open: boolean) => void;
  /** Optional callback when ingestion succeeds */
  onSuccess?: (response: WebIngestResponse) => void;
}

export function WebUrlDialog({
  open,
  onOpenChange,
  onSuccess,
}: WebUrlDialogProps) {
  const handleSuccess = (response: WebIngestResponse) => {
    // Close dialog on success
    onOpenChange(false);
    // Call parent callback
    onSuccess?.(response);
  };

  const handleCancel = () => {
    onOpenChange(false);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="Add Web URL"
      description="Extract and index content from any web page"
      showClose
      closeOnBackdrop
    >
      <WebUrlInput
        onSuccess={handleSuccess}
        onCancel={handleCancel}
        variant="standalone"
      />
    </Dialog>
  );
}

export default WebUrlDialog;
