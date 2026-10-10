/** The job kinds a batch import runs as; their reports carry the import's job id. */
export const BATCH_IMPORT_JOBS: ReadonlySet<string> = new Set(['batch.file_import', 'batch.url_import']);
