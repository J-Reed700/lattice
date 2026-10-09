/** The space every chat and document belongs to until the user picks another. */
export const GENERAL_SPACE_ID = 'space_general';

/** A space's accent as lowercase `#rgb`/`#rrggbb`, or null when it is not a hex color. */
export const normalizeHexColor = (value: string | null | undefined): string | null => {
  if (!value) return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  const candidate = trimmed.startsWith('#') ? trimmed : `#${trimmed}`;
  const isHex = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.test(candidate);
  return isHex ? candidate.toLowerCase() : null;
};
