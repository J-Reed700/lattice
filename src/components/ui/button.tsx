import * as React from "react"

import { Slot } from "@radix-ui/react-slot"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const buttonVariants = cva(
  "pressable inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-lg text-ui font-medium transition-[background-color,border-color,color,box-shadow,scale] duration-fast ease-out focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-bg disabled:pointer-events-none disabled:opacity-45 [&_svg]:pointer-events-none [&_svg]:size-[15px] [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default:
          "bg-action text-action-fg shadow-action hover:bg-action-hover",
        destructive:
          "bg-danger text-accent-fg shadow-action hover:bg-danger/90",
        outline:
          "border border-border-default bg-surface text-text-primary shadow-control hover:border-border-strong hover:bg-surface-raised",
        secondary:
          "bg-[hsl(var(--text-primary)/0.06)] text-text-primary hover:bg-[hsl(var(--text-primary)/0.1)]",
        ghost:
          "text-text-secondary hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-text-primary",
        link: "text-accent underline-offset-4 hover:underline",
      },
      size: {
        default: "h-8 px-3",
        sm: "h-7 rounded-md px-2.5 text-xs",
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
