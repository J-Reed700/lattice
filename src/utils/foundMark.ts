/** One citation mark in a run of document text, as a provider of marks reports it. */
export interface FoundMark {
  /** Offset of the mark in the text it was found in. */
  index: number;
  length: number;
  /** The source number the mark stands for. */
  number: number;
  /** True when the mark is a private glyph that must be hidden and written out when it has no source to open. */
  glyph: boolean;
}
