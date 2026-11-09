export type StatusState =
  | { kind: 'idle' }
  | { kind: 'busy'; label: string }
  | { kind: 'error'; message: string; retry?: () => void };

export function StatusLine(_props: { state: StatusState; className?: string }) {
  return null;
}
