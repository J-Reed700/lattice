import { type ReactNode } from 'react';

import { motion } from 'framer-motion';

import Button from '@/components/ui/Button/Button';
import { scaleIn, rotateIn } from '@/lib/animations';
import { cn } from '@/lib/utils';

export interface EmptyStateProps {
  icon: ReactNode;
  title: string;
  description: string;
  action?: {
    label: string;
    onClick: () => void;
  };
  className?: string;
}

export const EmptyState = ({
  icon,
  title,
  description,
  action,
  className,
}: EmptyStateProps) => (
    <motion.div
      variants={scaleIn}
      initial="hidden"
      animate="visible"
      className={cn(
        'flex flex-col items-center justify-center py-16 px-4 text-center',
        className
      )}
    >
      {/* Animated Icon Container */}
      <motion.div
        variants={rotateIn}
        initial="hidden"
        animate="visible"
        transition={{ delay: 0.1, type: 'spring', stiffness: 200, damping: 15 }}
        className="mb-6 p-6 rounded-full bg-accent-primary/10 dark:bg-accent-primary/20"
      >
        <div className="text-accent-primary w-12 h-12 flex items-center justify-center">
          {icon}
        </div>
      </motion.div>

      {/* Title */}
      <motion.h3
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.2 }}
        className="text-2xl font-bold text-text-primary mb-2"
      >
        {title}
      </motion.h3>

      {/* Description */}
      <motion.p
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.3 }}
        className="text-text-secondary max-w-md mb-6"
      >
        {description}
      </motion.p>

      {/* Action Button */}
      {action && (
        <motion.div
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.4 }}
        >
          <Button onClick={action.onClick} size="lg" variant="primary">
            {action.label}
          </Button>
        </motion.div>
      )}
    </motion.div>
  );
