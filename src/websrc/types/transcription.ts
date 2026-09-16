/**
 * On-device transcription types (Track E).
 *
 * Mirrors `features/transcription/dto.rs`. Audio files are transcribed during
 * ingest; `transcribeFile` is the explicit re-run.
 */

/** One contiguous run of transcribed speech. */
export interface TranscriptSegment {
  /** Offset of the first word from the start of the recording. */
  startMs: number;
  /** Offset of the last word from the start of the recording. */
  endMs: number;
  /** The spoken text. */
  text: string;
}

/** The transcript of one audio file. */
export interface Transcript {
  /** Absolute path of the transcribed file. */
  filePath: string;
  /** Language tag reported by the model. */
  language: string;
  /** Decoded audio duration. */
  durationMs: number;
  /** Segments in playback order. */
  segments: TranscriptSegment[];
  /** The rendered document text, with `[m:ss–m:ss]` window markers. */
  text: string;
}

/** Whether transcription is available right now. */
export interface TranscriptionStatus {
  /** True when a transcription model is downloaded. */
  modelReady: boolean;
  /** Display name of the model that would be used. */
  modelName?: string;
}
