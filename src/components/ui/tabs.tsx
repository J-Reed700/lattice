import * as React from "react"

import * as TabsPrimitive from "@radix-ui/react-tabs"

import { cn } from "@/lib/utils"

export type TabsVariant = "pill" | "underline"

const TabsVariantContext = React.createContext<TabsVariant>("pill")

export interface TabsProps
  extends React.ComponentPropsWithoutRef<typeof TabsPrimitive.Root> {
  /** Visual treatment propagated to child `TabsList` and `TabsTrigger` elements. */
  variant?: TabsVariant
}

const Tabs = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Root>,
  TabsProps
>(({ variant = "pill", ...props }, ref) => (
  <TabsVariantContext.Provider value={variant}>
    <TabsPrimitive.Root ref={ref} {...props} />
  </TabsVariantContext.Provider>
))
Tabs.displayName = TabsPrimitive.Root.displayName

export interface TabsListProps
  extends React.ComponentPropsWithoutRef<typeof TabsPrimitive.List> {
  /** Override the visual variant for this tab bar and its triggers. */
  variant?: TabsVariant
}

const TabsList = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.List>,
  TabsListProps
>(({ className, variant: variantProp, ...props }, ref) => {
  const contextVariant = React.useContext(TabsVariantContext)
  const variant = variantProp ?? contextVariant

  return (
    <TabsVariantContext.Provider value={variant}>
      <TabsPrimitive.List
        ref={ref}
        data-variant={variant}
        className={cn(
          variant === "underline"
            ? "inline-flex items-center gap-5 border-b border-border-subtle bg-transparent p-0 text-text-tertiary"
            : "inline-flex h-8 items-center justify-center gap-0.5 rounded-lg border border-border-subtle bg-surface-sunken p-[3px] text-text-tertiary",
          className
        )}
        {...props}
      />
    </TabsVariantContext.Provider>
  )
})
TabsList.displayName = TabsPrimitive.List.displayName

export interface TabsTriggerProps
  extends React.ComponentPropsWithoutRef<typeof TabsPrimitive.Trigger> {
  /** Override the visual variant for this trigger. */
  variant?: TabsVariant
}

const TabsTrigger = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Trigger>,
  TabsTriggerProps
>(({ className, variant: variantProp, ...props }, ref) => {
  const contextVariant = React.useContext(TabsVariantContext)
  const variant = variantProp ?? contextVariant

  return (
    <TabsPrimitive.Trigger
      ref={ref}
      data-variant={variant}
      className={cn(
        "inline-flex items-center justify-center gap-1.5 whitespace-nowrap text-ui font-medium focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50",
        variant === "underline"
          ? "relative -mb-px h-9 rounded-none border-b-2 border-transparent px-0.5 pb-2 pt-1.5 transition-colors duration-fast hover:text-text-primary focus-visible:rounded-sm data-[state=active]:border-accent data-[state=active]:text-text-primary"
          : "h-full rounded-md px-3 transition-[background-color,color,box-shadow] duration-fast hover:text-text-primary data-[state=active]:bg-surface data-[state=active]:text-text-primary data-[state=active]:shadow-sm data-[state=active]:ring-1 data-[state=active]:ring-border-subtle",
        className
      )}
      {...props}
    />
  )
})
TabsTrigger.displayName = TabsPrimitive.Trigger.displayName

const TabsContent = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.Content>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.Content
    ref={ref}
    className={cn(
      "mt-2 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-bg",
      className
    )}
    {...props}
  />
))
TabsContent.displayName = TabsPrimitive.Content.displayName

export { Tabs, TabsList, TabsTrigger, TabsContent }
