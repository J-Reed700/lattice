import React from 'react';

interface BadgeProps {
  children: React.ReactNode;
  variant?: 'default' | 'secondary' | 'outline' | 'destructive';
  className?: string;
}

export function Badge({ children, variant = 'default', className = '' }: BadgeProps) {
  const variantClasses = {
    default: 'bg-[var(--accent-primary)] text-white',
    secondary: 'bg-[var(--bg-tertiary)] text-[var(--text-primary)]',
    outline: 'border border-[var(--border-color)] text-[var(--text-secondary)]',
    destructive: 'bg-[var(--error)] text-white',
  };

  return (
    <span className={`inline-flex items-center px-2 py-1 rounded-full text-xs font-medium ${variantClasses[variant]} ${className}`}>
      {children}
    </span>
  );
}
