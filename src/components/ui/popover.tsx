import * as React from "react"

import * as PopoverPrimitive from "@radix-ui/react-popover"

import { cn } from "@/lib/utils"

const Popover = PopoverPrimitive.Root

const PopoverTrigger = PopoverPrimitive.Trigger

const PopoverAnchor = PopoverPrimitive.Anchor

const surfaceTokens = [
  '--bg', '--surface', '--surface-raised', '--surface-sunken', '--surface-overlay',
  '--border-subtle', '--border-default', '--border-strong', '--text-primary',
  '--text-secondary', '--text-tertiary', '--text-muted', '--accent', '--accent-hover',
  '--accent-muted', '--accent-fg', '--ring',
]

const PopoverContent = React.forwardRef<
  React.ElementRef<typeof PopoverPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof PopoverPrimitive.Content> & {
    /** Keep scoped theme tokens when a surface has its own palette. */
    portalContainer?: HTMLElement | null;
    /** Inherit a palette while keeping the portal outside layout containment. */
    inheritThemeFrom?: HTMLElement | null;
  }
>(({ className, align = "center", sideOffset = 6, portalContainer, inheritThemeFrom, style, ...props }, ref) => {
  const [tokens, setTokens] = React.useState<React.CSSProperties>({})
  React.useLayoutEffect(() => {
    if (!inheritThemeFrom) return
    const update = () => {
      const computed = getComputedStyle(inheritThemeFrom)
      setTokens(Object.fromEntries(surfaceTokens.map((token) => [token, computed.getPropertyValue(token)])) as React.CSSProperties)
    }
    update()
    const observer = new MutationObserver(update)
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class', 'data-theme', 'style'] })
    observer.observe(inheritThemeFrom, { attributes: true, attributeFilter: ['class', 'data-theme', 'style'] })
    return () => observer.disconnect()
  }, [inheritThemeFrom])
  return (
    <PopoverPrimitive.Portal container={portalContainer}>
      <PopoverPrimitive.Content
        ref={ref}
        align={align}
        sideOffset={sideOffset}
        style={{ ...tokens, ...style }}
        className={cn(
          "surface-pop z-50 w-72 rounded-lg bg-[hsl(var(--surface-overlay))] p-4 text-sm text-[hsl(var(--text-primary))] shadow-lg outline-hidden",
          className
        )}
        {...props}
      />
    </PopoverPrimitive.Portal>
  )
})
PopoverContent.displayName = PopoverPrimitive.Content.displayName

export { Popover, PopoverTrigger, PopoverAnchor, PopoverContent }
