/**
 * Icon
 *
 * Wrapper around any Lucide icon that auto-corrects stroke weight based on
 * rendered size. Small icons need thicker strokes to read; large icons need
 * thinner strokes or they look heavy. The default Lucide stroke (2px) is
 * tuned for ~24px — at 12px it reads like a smudge, at 32px it reads like
 * a marker.
 *
 * Usage:
 *   <Icon as={Cloud} size={16} />
 *   <Icon as={Trash2} size={12} className="text-danger-fg" />
 *
 * Don't pass `strokeWidth` directly unless you really mean to override the
 * optical correction.
 */

import { forwardRef } from 'react';
import type { LucideIcon, LucideProps } from 'lucide-react';

interface IconProps extends Omit<LucideProps, 'ref'> {
  /** The Lucide icon component, e.g. `Cloud`, `Trash2`. */
  as: LucideIcon;
  /** Pixel size. Defaults to 16 (the most common chrome icon size). */
  size?: number;
}

/**
 * Map rendered size to optically-corrected stroke weight.
 *
 * Reasoning:
 * - At 12px each pixel matters; 2.5 keeps glyphs legible.
 * - At 16-20px, default Lucide 2px works well.
 * - At ≥22px, drop to 1.5 so icons feel airy and elegant rather than blocky.
 */
function strokeForSize(size: number): number {
  if (size <= 14) return 2.5;
  if (size <= 20) return 2;
  return 1.5;
}

export const Icon = forwardRef<SVGSVGElement, IconProps>(
  ({ as: Component, size = 16, strokeWidth, ...rest }, ref) => {
    const stroke = strokeWidth ?? strokeForSize(size);
    return <Component ref={ref} size={size} strokeWidth={stroke} {...rest} />;
  },
);

Icon.displayName = 'Icon';
