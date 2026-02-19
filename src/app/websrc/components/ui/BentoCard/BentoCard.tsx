import { type ReactNode } from 'react';

import { motion } from 'framer-motion';

import { cn } from '@/lib/utils';

export interface BentoCardProps {
  children: ReactNode;
  className?: string;
  hover?: boolean;
}

/**
 * BentoCard - A simple card primitive for Bento Grid layouts
 *
 * Purpose: Minimal card component for modern grid-based layouts
 * Philosophy: Composition over configuration - keep it simple
 *
 * Usage:
 * <BentoCard className="col-span-2 bg-gradient-to-br from-sky-500/10 to-blue-500/10">
 *   <YourContent />
 * </BentoCard>
 */
export const BentoCard = ({
  children,
  className,
  hover = true,
}: BentoCardProps) => (
    <motion.div
      whileHover={hover ? { scale: 1.005, y: -2 } : {}}
      transition={{ type: 'spring', stiffness: 300, damping: 20 }}
      className={cn(
        // Base styles
        'rounded-2xl border bg-bg-secondary border-border-color',
        'p-6 transition-all duration-300',
        // Hover styles
        hover && 'shadow-sm hover:shadow-xl hover:border-[var(--border-hover)]',
        // Custom classes
        className
      )}
    >
      {children}
    </motion.div>
  );
