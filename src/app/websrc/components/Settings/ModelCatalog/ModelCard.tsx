/**
 * ModelCard
 *
 * Individual model card showing model metadata and compatibility
 */

import { Brain, HardDrive, Zap, CheckCircle2, Download, Heart } from 'lucide-react';

import Card from '../../ui/Card/Card';

import type { CompatibilityLevel, ModelRecommendation } from '../../../types/modelCatalog';

interface ModelCardProps {
  model: ModelRecommendation;
  onClick: () => void;
  isSelected?: boolean;
}

export function ModelCard({ model, onClick, isSelected = false }: ModelCardProps) {
  const { model: metadata, compatibility } = model;

  // Compatibility badge styling
  const getCompatibilityStyle = (level: CompatibilityLevel) => {
    switch (level) {
      case 'Excellent':
        return {
          bg: 'bg-[hsl(var(--success-muted))]',
          text: 'text-[hsl(var(--success-fg))]',
          icon: <CheckCircle2 className="w-3 h-3" />,
        };
      case 'Good':
        return {
          bg: 'bg-[hsl(var(--warning-muted))]',
          text: 'text-[hsl(var(--warning-fg))]',
          icon: <CheckCircle2 className="w-3 h-3" />,
        };
      case 'Poor':
        return {
          bg: 'bg-[hsl(var(--warning-muted))]',
          text: 'text-[hsl(var(--warning-fg))]',
          icon: null,
        };
      case 'Incompatible':
        return {
          bg: 'bg-[hsl(var(--danger-muted))]',
          text: 'text-[hsl(var(--danger-fg))]',
          icon: null,
        };
    }
  };

  const compatStyle = getCompatibilityStyle(compatibility.compatibility_level);
  const popularityDownloads = model.popularity_downloads ?? null;
  const popularityLikes = model.popularity_likes ?? null;

  const formattedDownloads = popularityDownloads
    ? new Intl.NumberFormat(undefined, {
        notation: 'compact',
        maximumFractionDigits: 1,
      }).format(popularityDownloads)
    : null;
  const formattedLikes = popularityLikes
    ? new Intl.NumberFormat(undefined, {
        notation: 'compact',
        maximumFractionDigits: 1,
      }).format(popularityLikes)
    : null;

  // Performance tier icon
  const getTierIcon = () => {
    switch (metadata.performance_tier) {
      case 'Fast':
        return <Zap className="w-3.5 h-3.5 text-[hsl(var(--success-fg))]" />;
      case 'Balanced':
        return <Brain className="w-3.5 h-3.5 text-[hsl(var(--accent))]" />;
      case 'Accurate':
        return <Brain className="w-3.5 h-3.5 text-[hsl(var(--accent))]" />;
    }
  };

  return (
    <Card
      variant="clickable"
      padding="md"
      onClick={onClick}
      className={`
        h-full min-h-[230px] text-left overflow-hidden transition-all
        ${isSelected ? 'ring-2 ring-[hsl(var(--accent))] ring-offset-2 shadow-[0_0_0_1px_hsl(var(--accent))]' : ''}
      `}
    >
      {/* Header with compatibility badge */}
      <div className="flex items-start justify-between gap-3 mb-3">
        <div className="flex-1 min-w-0">
          <h3 className="font-semibold text-[15px] leading-snug text-[hsl(var(--text-primary))] line-clamp-2 break-words">
            {metadata.name}
          </h3>
          <div className="flex items-center gap-1.5 mt-1">
            <p className="text-[11px] uppercase tracking-wide text-[hsl(var(--text-tertiary))]">
              {metadata.category}
            </p>
            {metadata.category === 'Embedding' && metadata.embedding_dimensions && (
              <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-secondary))]">
                {metadata.embedding_dimensions}d
              </span>
            )}
            {metadata.category === 'Embedding' &&
              metadata.embedding_compatibility &&
              metadata.embedding_compatibility.kind !== 'compatible' && (
                <span
                  className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-[hsl(var(--danger-muted))] text-[hsl(var(--danger-fg))]"
                  title={
                    metadata.embedding_compatibility.kind === 'incompatible'
                      ? metadata.embedding_compatibility.reason
                      : 'Architecture not recognized'
                  }
                >
                  Unsupported
                </span>
              )}
            {/* Format badge — only surfaced for non-default (non-GGUF)
                entries, since GGUF is the implicit norm for the catalog. */}
            {metadata.format === 'safetensors' && (
              <span
                className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent-fg))]"
                title={
                  `Safetensors format (HF native, unquantized). Loads via mistralrs ` +
                  `auto-detect. Needs ~${metadata.recommended_ram_gb} GB RAM — ` +
                  `significantly more than a GGUF quant of the same model.`
                }
              >
                Safetensors
              </span>
            )}
          </div>
        </div>
        <div className={`shrink-0 flex items-center gap-1 px-2.5 py-1 rounded-full text-xs font-medium whitespace-nowrap ${compatStyle.bg} ${compatStyle.text}`}>
          {compatStyle.icon}
          <span>{compatibility.compatibility_level}</span>
        </div>
      </div>

      {/* Description */}
      <p className="text-xs leading-relaxed text-[hsl(var(--text-secondary))] line-clamp-3 mb-3">
        {metadata.description}
      </p>

      {/* Metadata row */}
      <div className="grid grid-cols-3 gap-2 text-xs text-[hsl(var(--text-tertiary))]">
        <div className="min-w-0 flex items-center gap-1">
          <HardDrive className="w-3 h-3" />
          <span className="truncate tabular-nums">{metadata.size_gb > 0 ? `${metadata.size_gb.toFixed(1)} GB` : 'Unknown'}</span>
        </div>
        <div className="min-w-0 flex items-center gap-1">
          {getTierIcon()}
          <span className="truncate">{metadata.performance_tier}</span>
        </div>
        <div className="min-w-0 flex items-center justify-end gap-1">
          <span className="font-semibold tabular-nums">{compatibility.overall_score}</span>
          <span>/100</span>
        </div>
      </div>

      <div className="mt-2 flex items-center gap-3 text-[11px] text-[hsl(var(--text-tertiary))]">
        <span className="inline-flex items-center gap-1.5">
          <Download className="w-3 h-3" />
          {formattedDownloads ? `${formattedDownloads} downloads` : 'Downloads unavailable'}
        </span>
        <span className="inline-flex items-center gap-1.5">
          <Heart className="w-3 h-3" />
          {formattedLikes ? `${formattedLikes} likes` : 'Likes unavailable'}
        </span>
      </div>

      {/* Capabilities tags */}
      {metadata.capabilities.length > 0 && (
        <div className="flex flex-wrap gap-1.5 mt-3">
          {metadata.capabilities.slice(0, 3).map((capability) => (
            <span
              key={capability}
              className="max-w-full text-xs px-2 py-0.5 bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-secondary))] rounded-md truncate"
            >
              {capability}
            </span>
          ))}
          {metadata.capabilities.length > 3 && (
            <span className="text-xs px-2 py-0.5 text-[hsl(var(--text-tertiary))]">
              +{metadata.capabilities.length - 3}
            </span>
          )}
        </div>
      )}

      {/* Blockers if any */}
      {compatibility.blockers.length > 0 && (
        <div className="mt-3 pt-3 border-t border-[hsl(var(--border-subtle))]">
          <p className="text-xs text-[hsl(var(--danger-fg))] font-medium">
            ⚠️ {compatibility.blockers[0]}
          </p>
        </div>
      )}
    </Card>
  );
}
