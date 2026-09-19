import * as React from "react"

import { Slot } from "@radix-ui/react-slot"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const buttonVariants = cva(
  "pressable inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-md text-ui font-medium transition-[background-color,border-color,color,box-shadow,scale] duration-fast ease-out focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))] disabled:pointer-events-none disabled:opacity-45 [&_svg]:pointer-events-none [&_svg]:size-[15px] [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default:
          "bg-[hsl(var(--action))] text-[hsl(var(--action-fg))] shadow-action hover:bg-[hsl(var(--action-hover))]",
        destructive:
          "bg-[hsl(var(--danger))] text-white shadow-action hover:brightness-110",
        outline:
          "border border-[hsl(var(--border-default))] bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))] shadow-control hover:border-[hsl(var(--border-strong))] hover:bg-[hsl(var(--surface-raised))]",
        secondary:
          "bg-[hsl(var(--text-primary)/0.06)] text-[hsl(var(--text-primary))] hover:bg-[hsl(var(--text-primary)/0.1)]",
        ghost: "text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))]",
        link: "text-[hsl(var(--accent))] underline-offset-4 hover:underline",
      },
      size: {
        default: "h-8 px-3",
        sm: "h-7 rounded-[6px] px-2.5 text-xs",
        lg: "h-10 rounded-lg px-5 text-sm",
        icon: "h-8 w-8",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button"
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    )
  }
)
Button.displayName = "Button"

export { Button, buttonVariants }
