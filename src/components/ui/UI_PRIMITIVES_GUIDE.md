# UI Primitives Guide

This guide covers the Tooltip and DatePicker components in the Lattice desktop
app, and indexes the other primitives in `src/components/ui/`.

## Table of Contents

- [Tooltip Component](#tooltip-component)
- [DatePicker Component](#datepicker-component)
- [Other Primitives](#other-primitives)
- [Integration Examples](#integration-examples)
- [Best Practices](#best-practices)

---

## Tooltip Component

### Overview

`src/components/ui/tooltip.tsx` is a thin wrapper over Radix UI's tooltip.
`Tooltip`, `TooltipTrigger`, `TooltipProvider`, `TooltipPortal` and
`TooltipArrow` are the Radix parts re-exported as they are; `TooltipContent` is
the one styled part. There is no `content` prop: a tooltip is composed from its
parts.

For an icon-only button, use `IconButton` instead (see below): it renders the
tooltip for you from its `label`.

### Basic Usage

```tsx
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui';

function Example() {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button type="button">Hover me</button>
      </TooltipTrigger>
      <TooltipContent>This is helpful information</TooltipContent>
    </Tooltip>
  );
}
```

### Parts

| Part | Notable props | Description |
|------|---------------|-------------|
| `Tooltip` | `delayDuration`, `open`, `defaultOpen`, `onOpenChange` | Radix `Root`; holds open state |
| `TooltipTrigger` | `asChild` | The element that opens the tooltip; use `asChild` so your button is the trigger |
| `TooltipContent` | `side` (`'top' \| 'right' \| 'bottom' \| 'left'`, default `'top'`), `sideOffset` (default `6`), `align`, `className` | Styled content, rendered in a portal |
| `TooltipArrow` | | Optional arrow; `TooltipContent` draws none by default |

`TooltipProps` and `TooltipProviderProps` are re-exported from Radix.

### Advanced Usage

#### Icon Buttons: use IconButton

```tsx
import { IconButton } from '@/components/ui';
import { Trash2 } from 'lucide-react';

function DeleteButton() {
  return (
    <IconButton label="Delete document" shortcut="⌫" tooltipSide="bottom" onClick={onDelete}>
      <Trash2 />
    </IconButton>
  );
}
```

`IconButton` takes `label` (also its `aria-label`), optional `shortcut` (shown
as a `.kbd` in the tooltip), `size` (`'sm'` 28px, `'md'` 32px), `active`, and
`tooltipSide` (default `'bottom'`). Its tooltip opens after 300ms.

#### With Dynamic Content

```tsx
<Tooltip>
  <TooltipTrigger asChild>
    <button type="button" onClick={toggle}>
      {isOpen ? <ChevronUp /> : <ChevronDown />}
    </button>
  </TooltipTrigger>
  <TooltipContent>{isOpen ? 'Close panel' : 'Open panel'}</TooltipContent>
</Tooltip>
```

#### Label With Shortcut

```tsx
<Tooltip delayDuration={500}>
  <TooltipTrigger asChild>
    <button type="button" aria-label="Search">…</button>
  </TooltipTrigger>
  <TooltipContent side="right" sideOffset={10}>
    <span className="flex items-center gap-2">
      <span>Search</span>
      <kbd className="kbd">⌘1</kbd>
    </span>
  </TooltipContent>
</Tooltip>
```

### TooltipProvider

`src/App.tsx` already wraps the app in one `TooltipProvider` with Radix's
defaults. Tests that render a tooltip outside `App` need their own provider:

```tsx
import { TooltipProvider } from '@/components/ui';

render(
  <TooltipProvider>
    <ComponentUnderTest />
  </TooltipProvider>
);
```

#### TooltipProvider Props

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `children` | `React.ReactNode` | *required* | Your app content |
| `delayDuration` | `number` | `700` | Default delay for all tooltips (Radix default) |
| `skipDelayDuration` | `number` | `300` | Time before skipping delay when moving between tooltips |
| `disableHoverableContent` | `boolean` | `false` | Prevent hovering over tooltip content |

### Accessibility Features

- **Keyboard Navigation**: Tooltips appear on focus, dismiss with Escape
- **Screen Readers**: Radix links trigger and content via ARIA attributes
- **Labels**: A tooltip is not a label; icon-only buttons still need `aria-label` (`IconButton` sets it)

### Styling

`TooltipContent` is styled with theme tokens, so it follows light and dark
without `dark:` variants:

- `bg-[hsl(var(--surface-overlay))]`, `text-[hsl(var(--text-primary))]`, `text-xs font-medium`
- `rounded-[6px] px-2 py-1 shadow-lg`, `z-50`
- `surface-pop` (from `src/index.css`) for the open/close animation
- `pointer-events-none`: the content can't be clicked or hovered

Pass `className` to adjust it, e.g. `className="xl:hidden"` on the rail.

---

## DatePicker Component

### Overview

The DatePicker component provides a calendar popover for date selection.
Built with react-day-picker and date-fns, it includes keyboard navigation, date
constraints, and quick actions. No surface uses it yet.

### Basic Usage

```tsx
import { DatePicker } from '@/components/ui';
import { useState } from 'react';

function Example() {
  const [date, setDate] = useState<Date>();

  return (
    <DatePicker
      selected={date}
      onSelect={setDate}
      placeholder="Select a date"
    />
  );
}
```

### Props

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `selected` | `Date \| undefined` | - | Currently selected date |
| `onSelect` | `(date: Date \| undefined) => void` | *required* | Callback when date is selected |
| `disabled` | `boolean` | `false` | Disable the date picker |
| `placeholder` | `string` | `'Select date'` | Placeholder text when no date selected |
| `minDate` | `Date` | - | Minimum selectable date |
| `maxDate` | `Date` | - | Maximum selectable date |
| `className` | `string` | `''` | Additional CSS classes |
| `dateFormat` | `string` | `'MMM dd, yyyy'` | date-fns format for the selected date |

### Advanced Usage

#### Date Range Constraints

```tsx
import { DatePicker } from '@/components/ui';
import { addDays } from 'date-fns';

function BookingDatePicker() {
  const [date, setDate] = useState<Date>();
  const today = new Date();

  return (
    <DatePicker
      selected={date}
      onSelect={setDate}
      minDate={today} // Can't select past dates
      maxDate={addDays(today, 30)} // Only next 30 days
      placeholder="Select booking date"
    />
  );
}
```

#### From / To Pair

```tsx
function DateRangeFilter() {
  const [startDate, setStartDate] = useState<Date>();
  const [endDate, setEndDate] = useState<Date>();

  return (
    <div className="flex gap-4">
      <DatePicker selected={startDate} onSelect={setStartDate} maxDate={endDate} placeholder="Start date" />
      <DatePicker selected={endDate} onSelect={setEndDate} minDate={startDate} placeholder="End date" />
    </div>
  );
}
```

#### Custom Date Format

```tsx
<DatePicker
  selected={date}
  onSelect={setDate}
  dateFormat="yyyy-MM-dd" // ISO format
  placeholder="YYYY-MM-DD"
/>
```

### Features

#### Quick Actions

- **Today Button**: Quickly select today's date
- **Close Button**: Dismiss calendar without selecting
- **Clear Button**: Remove current selection (X icon on input)

#### Keyboard Navigation

- **Arrow Keys**: Navigate through days (react-day-picker)
- **Enter**: Select focused day
- **Escape**: Close calendar
- **Tab**: Navigate to action buttons

### Accessibility Features

- **ARIA Labels**: "Choose date", "Clear date", and a labelled calendar dialog
- **Focus Management**: Visible `ring` focus indicators
- **Click Outside**: Closes on outside click

### Styling

Colors come from theme tokens (`--surface`, `--text-primary`, `--ring`, …), so
the picker follows light and dark without `dark:` variants. Pass `className`
for layout:

```tsx
<DatePicker selected={date} onSelect={setDate} className="w-full" />
```

Calendar styling includes today's date, the selected date, disabled dates, and
hover states.

---

## Other Primitives

From the barrel, `@/components/ui`:

| Export | File | Notes |
|--------|------|-------|
| `Button` | `button.tsx` | |
| `Input` | `input/` | |
| `Card`, `CardHeader`, `CardTitle`, `CardDescription`, `CardContent` | `card.tsx` | |
| `Switch` | `switch.tsx` | |
| `Select` | `select.tsx` | Import `SelectTrigger`, `SelectContent`, `SelectItem`, `SelectValue` from `@/components/ui/select` |
| `Checkbox` | `Checkbox/` | |
| `Dialog` | `dialog.tsx` | |
| `Badge` | `badge.tsx` | |
| `Tabs`, `TabsList`, `TabsTrigger`, `TabsContent` | `tabs.tsx` | |
| `ScrollArea` | `ScrollArea/` | |
| `PageHeader`, `SectionHeading` | `PageHeader.tsx` | One header per page |
| `SettingsSection`, `SettingsRow`, `settingsFieldClass`, `settingsTextareaClass` | `SettingsSection.tsx` | Settings layout |
| `IconButton` | `IconButton.tsx` | Icon-only button with built-in tooltip |
| `SidebarHeader`, `SidebarSearch`, `SidebarTabs` | `SidebarHeader.tsx` | Sidebar chrome |
| `Toast`, `ToastContainer`, `useToast` | `Toast/` | Not the app's toasts: those are `src/components/Toast` with `@/hooks/useToast` |

Imported by path, not from the barrel: `popover.tsx` (`Popover`,
`PopoverTrigger`, `PopoverAnchor`, `PopoverContent`) and `skeleton.tsx` /
`Skeleton/`.

---

## Integration Examples

### 1. Navigation rail

Location: `src/components/Layout/Layout.tsx`

Each rail item is a `Tooltip` whose `TooltipContent` (`side="right"`,
`className="xl:hidden"`) shows the label and its ⌘-number shortcut; on wide
windows the rail shows labels and the tooltip is hidden.

### 2. Toolbars and sidebar headers

Location: `src/components/FileBrowser/LibraryToolbar.tsx`,
`src/components/FileBrowser/LibraryRail.tsx`, `src/components/Explorer/ExplorerChat.tsx`

```tsx
<IconButton label={isRailOpen ? 'Hide sidebar' : 'Show sidebar'} onClick={onToggleRail}>
  <PanelLeft />
</IconButton>
```

---

## Best Practices

### Tooltips

#### Do:

- Use `IconButton` for icon-only buttons; it labels and tooltips them together
- Keep tooltip content concise (1-2 lines)
- Include keyboard shortcuts in tooltip content when applicable
- Position tooltips to avoid covering important content
- Use consistent delay timing across your app

#### Don't:

- Don't use tooltips for essential information users must see
- Don't put interactive content in tooltips (use Popover instead; `TooltipContent` ignores the pointer)
- Don't use tooltips on disabled elements (they won't trigger)
- Don't duplicate button text in tooltip (add additional context instead)
- Avoid very long tooltip content (consider Dialog or modal)

### DatePicker

#### Do:

- Always provide a clear placeholder
- Set appropriate min/max dates for the context
- Use consistent date formatting across your app
- Consider timezone implications for your use case

#### Don't:

- Don't forget to handle undefined date (when user clears)
- Don't use DatePicker for time selection
- Don't forget to validate date ranges (start before end)
- Avoid unclear date formats (prefer readable formats)

### Performance Considerations

```tsx
// Good: one TooltipProvider at app level (App.tsx already has it)
<TooltipProvider>
  <App />
</TooltipProvider>

// Good: memoize date constraints
const minDate = useMemo(() => new Date(), []);
const maxDate = useMemo(() => addMonths(new Date(), 6), []);

<DatePicker selected={date} onSelect={setDate} minDate={minDate} maxDate={maxDate} />
```

---

## Dependencies

These components use the following packages (already installed; see
`package.json` for exact versions):

```json
{
  "@radix-ui/react-tooltip": "^1.2.8",
  "react-day-picker": "^9.11.1",
  "date-fns": "^3.6.0"
}
```

Also `react` 19, `lucide-react` for icons, and Tailwind 3 for styling.

---

## Troubleshooting

### Tooltip Not Showing

1. Ensure a `TooltipProvider` wraps the tree (true in the app; add one in tests)
2. Use `TooltipTrigger asChild` with an element that forwards refs
3. Check that `TooltipContent` isn't hidden by a responsive class (`xl:hidden`)
4. Check z-index conflicts

### DatePicker Calendar Not Opening

1. Ensure the calendar isn't clipped by parent overflow
2. Check z-index of parent containers
3. Verify `disabled` isn't set

### Style Issues

1. Use token classes, not raw colors; theme switching is driven by the `.dark` class (`darkMode: 'selector'` in `tailwind.config.js`)
2. Check for CSS specificity conflicts

---

## Additional Resources

- [Radix UI Tooltip Documentation](https://www.radix-ui.com/primitives/docs/components/tooltip)
- [React Day Picker Documentation](https://daypicker.dev/)
- [date-fns Documentation](https://date-fns.org/)

---

## Questions or Issues?

1. Check this guide for examples and best practices
2. Review the component source code in `src/components/ui/`
3. Consult the official documentation for dependencies
