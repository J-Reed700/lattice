import { format } from 'date-fns';

import { formatBytes as canonicalFormatBytes } from '@/utils/formatters';

export function formatBytes(bytes: number): string {
  return canonicalFormatBytes(bytes, { emptyForNonPositive: true });
}

export function formatBackupDate(createdAt: string): string | null {
  if (!createdAt) return null;
  const date = new Date(createdAt);
  if (Number.isNaN(date.getTime())) return null;
  return format(date, "MMM d, yyyy 'at' HH:mm");
}
