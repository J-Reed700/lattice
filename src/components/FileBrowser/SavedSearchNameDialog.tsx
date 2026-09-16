import { Button as InlineButton } from '../ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Input } from '../ui/input';

export type SavedSearchNameDialogMode = 'create' | 'rename';

interface SavedSearchNameDialogProps {
  open: boolean;
  mode: SavedSearchNameDialogMode | null;
  value: string;
  onChangeValue: (_value: string) => void;
  onClose: () => void;
  onSubmit: () => void;
}

export function SavedSearchNameDialog({
  open,
  mode,
  value,
  onChangeValue,
  onClose,
  onSubmit,
}: SavedSearchNameDialogProps) {
  const isCreate = mode === 'create';

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onClose();
        }
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {isCreate ? 'Save Search' : 'Rename Saved Search'}
          </DialogTitle>
          <DialogDescription>
            {isCreate
              ? 'Save the current query and filters as a reusable search preset.'
              : 'Update the name of this saved search.'}
          </DialogDescription>
        </DialogHeader>
        <Input
          value={value}
          onChange={(event) => onChangeValue(event.target.value)}
          placeholder="Search name"
          autoFocus
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              onSubmit();
            }
          }}
        />
        <DialogFooter>
          <InlineButton
            type="button"
            variant="outline"
            onClick={onClose}
          >
            Cancel
          </InlineButton>
          <InlineButton
            type="button"
            onClick={onSubmit}
          >
            {isCreate ? 'Save Search' : 'Rename'}
          </InlineButton>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
