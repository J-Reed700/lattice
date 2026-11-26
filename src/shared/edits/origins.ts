import type { EditOrigin } from './types';

const actionLabels = new Map<string, string>();

export function registerActionLabel(name: string, label: string): void {
  actionLabels.set(name, label);
}

export function originLabel(origin: EditOrigin): string {
  switch (origin.type) {
    case 'learner':
      return 'You edited this';
    case 'proposal':
      return `A suggestion from ${origin.model} that you accepted`;
    case 'action':
      return actionLabels.get(origin.name) ?? 'Changed by an action in the app';
    case 'revert':
      return 'You undid a change';
  }
}
