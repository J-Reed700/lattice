import * as React from "react"

import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const badgeVariants = cva(
  "inline-flex items-center rounded-md border px-2.5 py-0.5 text-xs font-semibold transition-colors duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]",
  {
    variants: {
      variant: {
        default:
          "border-transparent bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] hover:opacity-90",
        secondary:
          "border-transparent bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-primary))] hover:bg-[hsl(var(--surface))]",
        destructive:
          "border-transparent bg-[hsl(var(--danger))] text-[hsl(var(--accent-fg))] hover:opacity-90",
        outline: "border-[hsl(var(--border-subtle))] text-[hsl(var(--text-secondary))]",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
)

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

function Badge({ className, variant, ...props }: BadgeProps) {
  return (
    <div className={cn(badgeVariants({ variant }), className)} {...props} />
  )
}

export { Badge, badgeVariants }
