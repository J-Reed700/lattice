import type { ReactNode } from 'react';

import { Pencil } from 'lucide-react';

import { buttonVariants } from '@/components/ui/button';

import { useEditMode } from './editMode';

export function EditModeToggle(): ReactNode {
  const on = useEditMode((state) => state.on);
  const toggle = useEditMode((state) => state.toggle);

  return (
    <button
      type="button"
      aria-pressed={on}
      onClick={toggle}
      className={buttonVariants({ variant: 'outline' })}
    >
      <Pencil aria-hidden="true" />
      Edit mode
    </button>
  );
}
