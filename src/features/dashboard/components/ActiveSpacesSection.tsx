import { conversationUiStore, useConversationUiStore } from '@/features/chat/stores/conversationUiStore';
import type { DashboardSpaceSummary } from '@/features/dashboard/hooks/useDashboardQuery';
import { GENERAL_SPACE_ID, normalizeHexColor } from '@/features/spaces/model/spaces';
import { cn } from '@/lib/utils';
import { useOptionalConversationsStore } from '@/shared/conversations/conversationsStore';

const FALLBACK_COLORS = ['#b45309', '#0f766e', '#4338ca', '#be123c', '#4d7c0f', '#0369a1'] as const;
const MAX_HOME_SPACES = 6;

function dotColor(space: DashboardSpaceSummary, index: number): string {
  const normalized = normalizeHexColor(space.accentColor);
  if (normalized) return normalized;
  if (space.id === GENERAL_SPACE_ID) return 'hsl(var(--text-tertiary))';
  return FALLBACK_COLORS[index % FALLBACK_COLORS.length];
}

export interface ActiveSpacesSectionProps {
  spaces: readonly DashboardSpaceSummary[];
  onNavigate: (path: string) => void;
}

export function ActiveSpacesSection({ spaces, onNavigate }: ActiveSpacesSectionProps) {
  const selectedSpaceId = useConversationUiStore((state) => state.selectedSpaceId);
  const setSelectedSpace = useOptionalConversationsStore((state) => state.setSelectedSpace);

  if (spaces.length === 0) return null;

  const visible = spaces.slice(0, MAX_HOME_SPACES);

  const openSpace = (spaceId: string | null) => {
    if (setSelectedSpace) {
      setSelectedSpace(spaceId);
    } else {
      conversationUiStore.setState({ selectedSpaceId: spaceId });
    }
    onNavigate('/chat');
  };

  return (
    <section>
      <div className="mb-2 flex items-baseline justify-between px-1">
        <h2 className="text-ui font-medium text-text-tertiary">Spaces</h2>
        <button
          type="button"
          onClick={() => openSpace(null)}
          className="text-xs text-text-muted transition-colors duration-fast hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
        >
          All spaces
        </button>
      </div>

      <div className="flex flex-wrap gap-1.5">
        {visible.map((space, index) => {
          const isActive = selectedSpaceId === space.id;
          return (
            <button
              key={space.id}
              type="button"
              onClick={() => openSpace(space.id)}
              className={cn(
                'pressable inline-flex h-8 max-w-full items-center gap-2 rounded-lg border px-2.5 text-xs transition-[background-color,border-color,color,scale] duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                isActive
                  ? 'border-accent/35 bg-accent-muted font-medium text-text-primary'
                  : 'border-border-subtle bg-surface/70 text-text-secondary hover:border-border-default hover:bg-surface hover:text-text-primary',
              )}
            >
              {space.icon?.trim() ? (
                <span aria-hidden="true" className="text-[13px] leading-none">
                  {space.icon.trim()}
                </span>
              ) : (
                <span
                  aria-hidden="true"
                  className="h-2 w-2 shrink-0 rounded-full"
                  style={{ backgroundColor: dotColor(space, index) }}
                />
              )}
              <span className="truncate">{space.name}</span>
            </button>
          );
        })}
        {spaces.length > MAX_HOME_SPACES ? (
          <button
            type="button"
            onClick={() => openSpace(null)}
            className="inline-flex h-8 items-center rounded-lg border border-dashed border-border-subtle px-2.5 text-xs tabular-nums text-text-muted transition-colors duration-fast hover:border-border-default hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          >
            +{spaces.length - MAX_HOME_SPACES} more
          </button>
        ) : null}
      </div>
    </section>
  );
}
