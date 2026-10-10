import * as React from "react"

import * as DialogPrimitive from "@radix-ui/react-dialog"
import { X } from "lucide-react"

import { cn } from "@/lib/utils"

const Dialog = DialogPrimitive.Root

const DialogTrigger = DialogPrimitive.Trigger

const DialogPortal = DialogPrimitive.Portal

const DialogClose = DialogPrimitive.Close

const DialogOverlay = React.forwardRef<
  React.ElementRef<typeof DialogPrimitive.Overlay>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Overlay>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Overlay
    ref={ref}
    className={cn(
      "overlay-fade fixed inset-0 z-50 bg-[hsl(var(--overlay))]",
      className
    )}
    {...props}
  />
))
DialogOverlay.displayName = DialogPrimitive.Overlay.displayName

interface DialogContentProps extends React.ComponentPropsWithoutRef<typeof DialogPrimitive.Content> {
  /** For a dialog that draws its own way out, or none. */
  hideClose?: boolean
  /**
   * For a surface with its own position and look (the palette, a reader):
   * only `className` and `overlayClassName` apply.
   */
  unstyled?: boolean
  overlayClassName?: string
}

const DialogContent = React.forwardRef<
  React.ElementRef<typeof DialogPrimitive.Content>,
  DialogContentProps
>(({ className, children, hideClose = false, unstyled = false, overlayClassName, forceMount, onOpenAutoFocus, onCloseAutoFocus, ...props }, ref) => {
  // Radix returns focus only to a `DialogTrigger`. A dialog opened any other
  // way returns it to whatever had it when the dialog opened, unless the
  // caller places it itself.
  const returnFocusRef = React.useRef<HTMLElement | null>(null)
  return (
    // `forceMount` keeps the whole dialog mounted for a caller that animates
    // its own exit (framer-motion's AnimatePresence).
    <DialogPortal forceMount={forceMount}>
      {unstyled
        ? <DialogPrimitive.Overlay forceMount={forceMount} className={overlayClassName} />
        : <DialogOverlay forceMount={forceMount} className={overlayClassName} />}
      <DialogPrimitive.Content
        ref={ref}
        forceMount={forceMount}
        className={unstyled ? className : cn(
          "surface-pop fixed left-1/2 top-1/2 z-50 grid w-[calc(100%-32px)] max-w-lg grid-cols-[minmax(0,1fr)] gap-4 rounded-xl bg-[hsl(var(--surface-overlay))] p-6 shadow-lg outline-hidden [translate:-50%_-50%]",
          className
        )}
        onOpenAutoFocus={(event) => {
          const active = document.activeElement
          returnFocusRef.current = active instanceof HTMLElement && active !== document.body ? active : null
          onOpenAutoFocus?.(event)
        }}
        onCloseAutoFocus={(event) => {
          onCloseAutoFocus?.(event)
          if (event.defaultPrevented) return
          event.preventDefault()
          if (returnFocusRef.current?.isConnected) returnFocusRef.current.focus()
        }}
        {...props}
      >
        {/* Alone, so `asChild` gets the single element it merges onto. */}
        {hideClose ? children : (
          <>
            {children}
            <DialogPrimitive.Close className="absolute right-3.5 top-3.5 inline-flex h-7 w-7 items-center justify-center rounded-md text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))] focus:outline-hidden focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] disabled:pointer-events-none">
              <X className="h-4 w-4" strokeWidth={1.75} />
              <span className="sr-only">Close</span>
            </DialogPrimitive.Close>
          </>
        )}
      </DialogPrimitive.Content>
    </DialogPortal>
  )
})
DialogContent.displayName = DialogPrimitive.Content.displayName

const DialogHeader = ({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) => (
  <div
    className={cn(
      "flex flex-col space-y-1.5 text-center sm:text-left",
      className
    )}
    {...props}
  />
)
DialogHeader.displayName = "DialogHeader"

const DialogFooter = ({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) => (
  <div
    className={cn(
      "flex flex-col-reverse gap-2 sm:flex-row sm:justify-end",
      className
    )}
    {...props}
  />
)
DialogFooter.displayName = "DialogFooter"

const DialogTitle = React.forwardRef<
  React.ElementRef<typeof DialogPrimitive.Title>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Title>
>(({ className, asChild, ...props }, ref) => (
  <DialogPrimitive.Title
    ref={ref}
    asChild={asChild}
    // With `asChild` the element passed in is the title, styled as it is.
    className={asChild ? className : cn(
      "font-serif text-xl font-medium leading-snug tracking-[-0.015em] text-[hsl(var(--text-primary))]",
      className
    )}
    {...props}
  />
))
DialogTitle.displayName = DialogPrimitive.Title.displayName

const DialogDescription = React.forwardRef<
  React.ElementRef<typeof DialogPrimitive.Description>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Description>
>(({ className, asChild, ...props }, ref) => (
  <DialogPrimitive.Description
    ref={ref}
    asChild={asChild}
    className={asChild ? className : cn("text-sm leading-relaxed text-[hsl(var(--text-tertiary))]", className)}
    {...props}
  />
))
DialogDescription.displayName = DialogPrimitive.Description.displayName

export {
  Dialog,
  DialogPortal,
  DialogOverlay,
  DialogTrigger,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogFooter,
  DialogTitle,
  DialogDescription,
}
