import { formatOfExtension, INDEXABLE_EXTENSIONS, isIndexableFormat } from './fileTypeCatalogue';

export type ValidationResult =
  | { ok: true; extension: string }
  | { ok: false; reason: string; extension?: string };

export function getExtensionFromPath(path: string): string | null {
  const fileName = path.split(/[\\/]/).pop() ?? path;
  const dotIndex = fileName.lastIndexOf('.');
  if (dotIndex <= 0 || dotIndex === fileName.length - 1) {
    return null;
  }
  return fileName.slice(dotIndex + 1).toLowerCase();
}

export function validateIndexablePath(path: string): ValidationResult {
  const extension = getExtensionFromPath(path);
  if (!extension) {
    return { ok: false, reason: 'Missing file extension' };
  }
  const format = formatOfExtension(extension);
  if (format?.category === 'archive' || format?.category === 'binary') {
    return {
      ok: false,
      reason: 'Executables and archives are not supported',
      extension,
    };
  }
  if (!isIndexableFormat(format)) {
    return { ok: false, reason: 'Unsupported file type', extension };
  }
  return { ok: true, extension };
}

export function filterIndexablePaths(paths: string[]): {
  accepted: string[];
  rejected: Array<{ path: string; reason: string; extension?: string }>;
} {
  const accepted: string[] = [];
  const rejected: Array<{ path: string; reason: string; extension?: string }> = [];

  for (const path of paths) {
    const validation = validateIndexablePath(path);
    if (validation.ok) {
      accepted.push(path);
    } else {
      rejected.push({
        path,
        reason: validation.reason,
        extension: validation.extension,
      });
    }
  }

  return { accepted, rejected };
}

export const SUPPORTED_EXTENSIONS_LIST = [...INDEXABLE_EXTENSIONS];
