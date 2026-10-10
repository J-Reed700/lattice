import type { ExplorerFocusDto, ExplorerLineRangeDto } from '@/lib/bindings';

/**
 * The surface that shows a folder thread's files. Chat reaches it only through
 * this: what a turn from the thread carries, and where a line reference in an
 * answer opens. The Explorer registers itself; with nothing registered a turn
 * carries no focus and a line reference opens nothing.
 */
export interface FolderThreadHost {
  /** The open file and selected lines while the surface shows `root`, else null. */
  focusFor: (root: string) => ExplorerFocusDto | null;
  /** Open a file and scroll its lines into view. */
  reveal: (path: string, range: ExplorerLineRangeDto) => void;
  /** Open a file at its top. */
  openFile: (path: string) => void;
}

let host: FolderThreadHost | null = null;

export function setFolderThreadHost(next: FolderThreadHost | null): void {
  host = next;
}

export function folderFocusFor(root: string): ExplorerFocusDto | null {
  return host?.focusFor(root) ?? null;
}

export function revealInFolder(path: string, range: ExplorerLineRangeDto): void {
  host?.reveal(path, range);
}

export function openInFolder(path: string): void {
  host?.openFile(path);
}
