/**
 * On-device transcription types.
 *
 * Mirrors `features/transcription/dto.rs`. Audio files are transcribed during
 * ingest; `transcribeFile` is the explicit re-run.
 */

/** One contiguous run of transcribed speech. */
export type TranscriptSegment = import('../lib/bindings').TranscriptSegmentDto;

/** The transcript of one audio file. */
export type Transcript = import('../lib/bindings').TranscriptDto;

/** Whether transcription is available right now. */
export type TranscriptionStatus = import('../lib/bindings').TranscriptionStatusDto;
