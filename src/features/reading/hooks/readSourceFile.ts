import VaultAPI from '@/lib/api';

/** A source file's text, for the readers that render text, code and saved pages. */
export const readSourceText = (path: string) => VaultAPI.readFileContent(path);

/** A source file's bytes, for the readers that parse a binary format (PDF, DOCX). */
export const readSourceBytes = (path: string) => VaultAPI.readFileBytes(path);
