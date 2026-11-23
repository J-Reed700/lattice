import type { ConversationSpaceDto } from '@/types';

export const RAIL_SPACE_IDS_KEY = 'layout.railSpaceIds';
export const MAX_RAIL_SPACES = 5;

/**
 * Reads user-pinned sidebar space IDs from localStorage.
 * Returns `null` when the user has not customized pins (use default order).
 */
export function readPinnedRailSpaceIds(): string[] | null {
  try {
    const raw = localStorage.getItem(RAIL_SPACE_IDS_KEY);
    if (raw === null) return null;
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return null;
    const unique: string[] = [];
    for (const item of parsed) {
      if (typeof item === 'string' && item.trim() && !unique.includes(item)) {
        unique.push(item);
        if (unique.length >= MAX_RAIL_SPACES) break;
      }
    }
    return unique;
  } catch {
    return null;
  }
}

export function writePinnedRailSpaceIds(ids: string[] | null): void {
  try {
    if (ids === null) {
      localStorage.removeItem(RAIL_SPACE_IDS_KEY);
      return;
    }
    localStorage.setItem(RAIL_SPACE_IDS_KEY, JSON.stringify(ids.slice(0, MAX_RAIL_SPACES)));
  } catch {
    // Preference only.
  }
}

export interface ResolvedRailSpaces {
  /** Spaces rendered directly in the left rail (capped at MAX_RAIL_SPACES). */
  visible: ConversationSpaceDto[];
  /** Effective pinned IDs shown in the sidebar (matches `visible`). */
  visibleIds: Set<string>;
  /** Count of open spaces not shown directly in the left rail. */
  hiddenCount: number;
  /** Whether the user has a custom pin configuration active. */
  isCustomized: boolean;
}

/**
 * Resolves which open spaces appear in the left rail, always capping at 5.
 * Falls back gracefully to the first 5 spaces if all saved pins were archived/deleted.
 */
export function resolveVisibleRailSpaces(
  openSpaces: readonly ConversationSpaceDto[],
  pinnedIds: readonly string[] | null,
): ResolvedRailSpaces {
  if (openSpaces.length === 0) {
    return { visible: [], visibleIds: new Set(), hiddenCount: 0, isCustomized: pinnedIds !== null };
  }

  if (pinnedIds !== null) {
    const byId = new Map(openSpaces.map((space) => [space.id, space]));
    const matched: ConversationSpaceDto[] = [];
    for (const id of pinnedIds) {
      const space = byId.get(id);
      if (space && !matched.some((existing) => existing.id === space.id)) {
        matched.push(space);
        if (matched.length >= MAX_RAIL_SPACES) break;
      }
    }
    if (matched.length > 0) {
      return {
        visible: matched,
        visibleIds: new Set(matched.map((space) => space.id)),
        hiddenCount: Math.max(0, openSpaces.length - matched.length),
        isCustomized: true,
      };
    }
  }

  const defaults = openSpaces.slice(0, MAX_RAIL_SPACES);
  return {
    visible: defaults,
    visibleIds: new Set(defaults.map((space) => space.id)),
    hiddenCount: Math.max(0, openSpaces.length - defaults.length),
    isCustomized: false,
  };
}

/**
 * Toggles a space's pinned status in the sidebar, honoring the 5-space cap.
 * Returns `{ next, atLimit }` where `atLimit` is true when trying to pin a 6th space.
 */
export function togglePinnedRailSpace(
  spaceId: string,
  openSpaces: readonly ConversationSpaceDto[],
  currentPinnedIds: readonly string[] | null,
): { next: string[]; atLimit: boolean } {
  const validIds = new Set(openSpaces.map((space) => space.id));
  const baseline =
    currentPinnedIds !== null
      ? currentPinnedIds.filter((id) => validIds.has(id)).slice(0, MAX_RAIL_SPACES)
      : openSpaces.slice(0, MAX_RAIL_SPACES).map((space) => space.id);

  if (baseline.includes(spaceId)) {
    return { next: baseline.filter((id) => id !== spaceId), atLimit: false };
  }

  if (baseline.length >= MAX_RAIL_SPACES) {
    return { next: baseline, atLimit: true };
  }

  return { next: [...baseline, spaceId], atLimit: false };
}
