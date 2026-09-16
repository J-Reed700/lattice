import React from 'react';

interface BadgeProps {
  children: React.ReactNode;
  variant?: 'default' | 'secondary' | 'outline' | 'destructive';
  className?: string;
}

export function Badge({ children, variant = 'default', className = '' }: BadgeProps) {
  const variantClasses = {
    default: 'bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))]',
    secondary: 'bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-primary))]',
    outline: 'border border-[hsl(var(--border-subtle))] text-[hsl(var(--text-secondary))]',
    destructive: 'bg-[hsl(var(--danger))] text-[hsl(var(--accent-fg))]',
  };

  return (
    <span className={`inline-flex items-center px-2 py-1 rounded-full text-xs font-medium ${variantClasses[variant]} ${className}`}>
      {children}
    </span>
  );
}
