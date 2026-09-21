import * as React from "react"

import * as SwitchPrimitives from "@radix-ui/react-switch"

import { cn } from "@/lib/utils"

const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitives.Root>,
  React.ComponentPropsWithoutRef<typeof SwitchPrimitives.Root>
>(({ className, ...props }, ref) => (
  <SwitchPrimitives.Root
    className={cn(
      "group peer inline-flex h-[20px] w-[34px] shrink-0 cursor-pointer items-center rounded-full p-[2px] shadow-[inset_0_1px_2px_rgb(0_0_0/0.18)] transition-colors duration-base ease-out focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))] disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-[hsl(var(--accent))] data-[state=unchecked]:bg-[hsl(var(--text-primary)/0.16)]",
      className
    )}
    {...props}
    ref={ref}
  >
    <SwitchPrimitives.Thumb
      className={cn(
        "pointer-events-none block h-4 w-4 rounded-full bg-white shadow-[0_1px_2px_rgb(0_0_0/0.35)] transition-[transform,width] duration-base ease-out group-active:w-[19px] data-[state=checked]:translate-x-[14px] data-[state=unchecked]:translate-x-0 group-active:data-[state=checked]:translate-x-[11px]"
      )}
    />
  </SwitchPrimitives.Root>
))
Switch.displayName = SwitchPrimitives.Root.displayName

export { Switch }
