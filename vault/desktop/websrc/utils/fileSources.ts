const ASSET_PROTOCOL_PREFIXES = [
  'asset://localhost/',
  'tauri://localhost/',
  'https://asset.localhost/',
  'http://asset.localhost/',
];

export function isRemoteFileSource(source: string): boolean {
  return /^(https?:|data:|blob:)/i.test(source) && !/^https?:\/\/asset\.localhost\//i.test(source);
}

export function resolveLocalPathFromViewerSource(source: string): string {
  const trimmed = source.trim();

  for (const prefix of ASSET_PROTOCOL_PREFIXES) {
    if (trimmed.startsWith(prefix)) {
      const rawPath = trimmed.slice(prefix.length);

      try {
        const decodedPath = decodeURIComponent(rawPath);
        return decodedPath.startsWith('/') ? decodedPath : `/${decodedPath}`;
      } catch {
        return rawPath.startsWith('/') ? rawPath : `/${rawPath}`;
      }
    }
  }

  if (trimmed.startsWith('file://')) {
    try {
      return decodeURIComponent(new URL(trimmed).pathname);
    } catch {
      return trimmed;
    }
  }

  return trimmed;
}
