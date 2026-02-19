# UI Primitives Guide

This guide covers the Tooltip and DatePicker components added to the Vault desktop app.

## Table of Contents

- [Tooltip Component](#tooltip-component)
- [DatePicker Component](#datepicker-component)
- [Integration Examples](#integration-examples)
- [Best Practices](#best-practices)

---

## Tooltip Component

### Overview

The Tooltip component displays contextual information when users hover over or focus on an element. Built with Radix UI's tooltip primitive, it provides excellent accessibility and customization options.

### Basic Usage

```tsx
import { Tooltip } from '@/components/ui';

function Example() {
  return (
    <Tooltip content="This is helpful information">
      <button>Hover me</button>
    </Tooltip>
  );
}
```

### Props

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `children` | `React.ReactNode` | *required* | The element that triggers the tooltip |
| `content` | `string \| React.ReactNode` | *required* | Content to display in the tooltip |
| `side` | `'top' \| 'right' \| 'bottom' \| 'left'` | `'top'` | Preferred side to display tooltip |
| `delay` | `number` | `200` | Delay in ms before showing tooltip |
| `disabled` | `boolean` | `false` | Disable the tooltip |
| `sideOffset` | `number` | `8` | Distance from trigger element in pixels |

### Advanced Usage

#### With Icon Buttons

```tsx
import { Tooltip } from '@/components/ui';
import { Trash2 } from 'lucide-react';

function DeleteButton() {
  return (
    <Tooltip content="Delete document" side="bottom">
      <button className="p-2 hover:bg-gray-100 rounded">
        <Trash2 className="h-4 w-4" />
      </button>
    </Tooltip>
  );
}
```

#### With Dynamic Content

```tsx
<Tooltip content={isOpen ? 'Close panel' : 'Open panel'}>
  <button onClick={toggle}>
    {isOpen ? <ChevronUp /> : <ChevronDown />}
  </button>
</Tooltip>
```

#### Rich Content Tooltip

```tsx
<Tooltip
  content={
    <div className="space-y-1">
      <p className="font-semibold">Keyboard Shortcut</p>
      <p className="text-sm">Press Cmd+K</p>
    </div>
  }
  side="right"
>
  <button>Search</button>
</Tooltip>
```

### TooltipProvider

Wrap your app (or a section) with `TooltipProvider` to configure global tooltip behavior:

```tsx
import { TooltipProvider } from '@/components/ui';

function App() {
  return (
    <TooltipProvider delayDuration={200} skipDelayDuration={300}>
      <YourApp />
    </TooltipProvider>
  );
}
```

#### TooltipProvider Props

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `children` | `React.ReactNode` | *required* | Your app content |
| `delayDuration` | `number` | `200` | Default delay for all tooltips |
| `skipDelayDuration` | `number` | `300` | Time before skipping delay when moving between tooltips |
| `disableHoverableContent` | `boolean` | `false` | Prevent hovering over tooltip content |

### Accessibility Features

- **Keyboard Navigation**: Tooltips appear on focus, dismiss with Escape
- **Screen Readers**: Content is announced via ARIA attributes
- **Reduced Motion**: Respects `prefers-reduced-motion` for animations
- **Touch Devices**: Works on touch with appropriate delays

### Styling

Tooltips use Tailwind classes and support dark mode:

```tsx
// Dark background with white text (default)
<Tooltip content="Dark tooltip">
  <button>Hover</button>
</Tooltip>

// The styling is handled internally with:
// - Dark mode support (dark:bg-gray-800)
// - Animations (fade-in, zoom-in)
// - Arrow indicator
// - Shadow for depth
```

---

## DatePicker Component

### Overview

The DatePicker component provides an intuitive calendar interface for date selection. Built with react-day-picker and date-fns, it includes keyboard navigation, date constraints, and quick actions.

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
| `dateFormat` | `string` | `'MMM dd, yyyy'` | Format for displaying selected date |

### Advanced Usage

#### Date Range Constraints

```tsx
import { DatePicker } from '@/components/ui';
import { addDays, subDays } from 'date-fns';

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

#### Search Filters with Date Range

```tsx
function SearchFilters() {
  const [startDate, setStartDate] = useState<Date>();
  const [endDate, setEndDate] = useState<Date>();

  return (
    <div className="flex gap-4">
      <div>
        <label className="block text-sm font-medium mb-2">From</label>
        <DatePicker
          selected={startDate}
          onSelect={setStartDate}
          maxDate={endDate} // End date is the max
          placeholder="Start date"
        />
      </div>
      <div>
        <label className="block text-sm font-medium mb-2">To</label>
        <DatePicker
          selected={endDate}
          onSelect={setEndDate}
          minDate={startDate} // Start date is the min
          placeholder="End date"
        />
      </div>
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

<DatePicker
  selected={date}
  onSelect={setDate}
  dateFormat="MMMM d, yyyy" // Long format: January 1, 2025
  placeholder="Month Day, Year"
/>
```

#### Form Integration

```tsx
function EventForm() {
  const [formData, setFormData] = useState({
    title: '',
    eventDate: undefined as Date | undefined,
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    console.log('Event:', formData);
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      <Input
        value={formData.title}
        onChange={(e) => setFormData({ ...formData, title: e.target.value })}
        placeholder="Event title"
      />

      <DatePicker
        selected={formData.eventDate}
        onSelect={(date) => setFormData({ ...formData, eventDate: date })}
        placeholder="Event date"
        minDate={new Date()}
      />

      <Button type="submit">Create Event</Button>
    </form>
  );
}
```

### Features

#### Quick Actions

- **Today Button**: Quickly select today's date
- **Close Button**: Dismiss calendar without selecting
- **Clear Button**: Remove current selection (X icon on input)

#### Keyboard Navigation

- **Arrow Keys**: Navigate through days
- **Enter**: Select focused day
- **Escape**: Close calendar
- **Tab**: Navigate to action buttons

#### Touch Support

- Optimized for mobile with appropriate touch targets
- Smooth scrolling and interactions
- Responsive calendar layout

### Accessibility Features

- **WCAG AA Compliant**: Proper contrast ratios
- **Keyboard Accessible**: Full keyboard navigation support
- **ARIA Labels**: Proper labeling for screen readers
- **Focus Management**: Visible focus indicators
- **Click Outside**: Close on outside click

### Styling

The DatePicker includes comprehensive styling with dark mode support:

```tsx
// The component automatically adapts to your theme
<DatePicker
  selected={date}
  onSelect={setDate}
  className="w-full" // Add custom classes
/>
```

Calendar styling includes:
- Today's date highlighting
- Selected date emphasis
- Disabled date styling
- Hover states
- Dark mode variants

---

## Integration Examples

### 1. Theme Toggle with Tooltips

Location: `src/components/ThemeToggle/ThemeToggle.tsx`

```tsx
import { Tooltip } from '../ui';

export function ThemeToggle() {
  return (
    <div className="flex gap-1">
      {options.map((option) => (
        <Tooltip
          key={option.value}
          content={`Switch to ${option.label.toLowerCase()} theme`}
          side="bottom"
        >
          <button onClick={() => setTheme(option.value)}>
            {option.icon}
          </button>
        </Tooltip>
      ))}
    </div>
  );
}
```

### 2. Document Viewer Actions with Tooltips

Location: `src/components/DocumentViewer/ViewerHeader.tsx`

```tsx
import { Tooltip } from '../ui';
import { Download, ExternalLink, X } from 'lucide-react';

export function ViewerHeader() {
  return (
    <div className="flex gap-2">
      <Tooltip content="Show in folder" side="bottom">
        <button onClick={onDownload}>
          <Download className="w-5 h-5" />
        </button>
      </Tooltip>

      <Tooltip content="Open in external application" side="bottom">
        <button onClick={onOpenExternal}>
          <ExternalLink className="w-5 h-5" />
        </button>
      </Tooltip>

      <Tooltip content="Close viewer (Esc)" side="bottom">
        <button onClick={onClose}>
          <X className="w-5 h-5" />
        </button>
      </Tooltip>
    </div>
  );
}
```

### 3. Settings with Tooltips

Location: `src/components/Settings/tabs/IndexingSettings.tsx`

```tsx
import { Tooltip } from '../../ui';
import { Trash2 } from 'lucide-react';

export function IndexingSettings() {
  return (
    <div>
      {folders.map((folder) => (
        <div key={folder} className="flex justify-between">
          <span>{folder}</span>
          <Tooltip content="Remove folder from indexing" side="left">
            <button onClick={() => removeFolder(folder)}>
              <Trash2 className="w-4 h-4" />
            </button>
          </Tooltip>
        </div>
      ))}
    </div>
  );
}
```

### 4. Search Results Date Filter

```tsx
import { DatePicker } from '../ui';

function SearchFilters() {
  const [startDate, setStartDate] = useState<Date>();
  const [endDate, setEndDate] = useState<Date>();

  return (
    <div className="flex gap-4">
      <DatePicker
        selected={startDate}
        onSelect={setStartDate}
        placeholder="From date"
        maxDate={endDate}
      />
      <DatePicker
        selected={endDate}
        onSelect={setEndDate}
        placeholder="To date"
        minDate={startDate}
      />
    </div>
  );
}
```

---

## Best Practices

### Tooltips

#### Do:

- Use tooltips for icon-only buttons to clarify their purpose
- Keep tooltip content concise (1-2 lines)
- Include keyboard shortcuts in tooltip content when applicable
- Position tooltips to avoid covering important content
- Use consistent delay timing across your app

#### Don't:

- Don't use tooltips for essential information users must see
- Don't put interactive content in tooltips (use Popover instead)
- Don't use tooltips on disabled elements (they won't trigger)
- Don't duplicate button text in tooltip (add additional context instead)
- Avoid very long tooltip content (consider Dialog or modal)

### DatePicker

#### Do:

- Always provide a clear placeholder
- Set appropriate min/max dates for the context
- Use consistent date formatting across your app
- Provide validation feedback for invalid selections
- Consider timezone implications for your use case

#### Don't:

- Don't forget to handle undefined date (when user clears)
- Don't use DatePicker for time selection (use separate time picker)
- Don't make date constraints too restrictive
- Don't forget to validate date ranges (start before end)
- Avoid unclear date formats (prefer readable formats)

### Performance Considerations

#### Tooltip Optimization

```tsx
// Good: Memoize expensive tooltip content
const tooltipContent = useMemo(() => (
  <ComplexTooltipContent data={data} />
), [data]);

<Tooltip content={tooltipContent}>
  <button>Hover</button>
</Tooltip>

// Good: Use TooltipProvider at app level for better performance
<TooltipProvider>
  <App />
</TooltipProvider>
```

#### DatePicker Optimization

```tsx
// Good: Memoize date constraints
const minDate = useMemo(() => new Date(), []);
const maxDate = useMemo(() => addMonths(new Date(), 6), []);

<DatePicker
  selected={date}
  onSelect={setDate}
  minDate={minDate}
  maxDate={maxDate}
/>
```

---

## Dependencies

These components require the following packages (already installed):

```json
{
  "@radix-ui/react-tooltip": "^1.0.7",
  "react-day-picker": "^8.10.0",
  "date-fns": "^3.0.0"
}
```

### Peer Dependencies

- `react` >= 18.0.0
- `react-dom` >= 18.0.0
- `lucide-react` (for icons)
- `tailwindcss` (for styling)

---

## Troubleshooting

### Tooltip Not Showing

1. Ensure `TooltipProvider` wraps your component tree
2. Check that trigger element accepts ref forwarding
3. Verify tooltip isn't disabled
4. Check z-index conflicts

### DatePicker Calendar Not Opening

1. Ensure calendar popover isn't clipped by parent overflow
2. Check z-index of parent containers
3. Verify click handler isn't prevented
4. Check for JavaScript errors in console

### Style Issues

1. Ensure Tailwind classes are properly configured
2. Check for CSS specificity conflicts
3. Verify dark mode class is applied to root element
4. Check for missing Tailwind plugin configurations

---

## Additional Resources

- [Radix UI Tooltip Documentation](https://www.radix-ui.com/docs/primitives/components/tooltip)
- [React Day Picker Documentation](https://react-day-picker.js.org/)
- [date-fns Documentation](https://date-fns.org/)
- [WCAG Tooltip Guidelines](https://www.w3.org/WAI/WCAG21/Understanding/)

---

## Questions or Issues?

If you encounter issues or have questions about these components, please:

1. Check this guide for examples and best practices
2. Review the component source code in `src/components/ui/`
3. Consult the official documentation for dependencies
4. Check existing issues in the project repository

---

*Last updated: 2025-01-11*
