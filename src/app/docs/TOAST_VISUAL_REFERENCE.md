# Toast Visual Reference

Visual guide to toast notification appearance and behavior.

## Toast Anatomy

```
┌─────────────────────────────────────────────────────┐
│  [Icon]  Title Text                            [X]  │
│          Optional message text that provides        │
│          additional context or details              │
│          [Action Button]                            │
│  ━━━━━━━━━━━━━━━━━━━━━━━━                          │ ← Progress bar
└─────────────────────────────────────────────────────┘
```

## Toast Types

### Success Toast

**Color**: Green
**Icon**: Checkmark circle
**Use**: Confirm successful actions

```
┌─────────────────────────────────────────────────────┐
│  ✓   Settings saved successfully              ×   │
│  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━          │
└─────────────────────────────────────────────────────┘
```

**Example Uses**:
- File uploaded successfully
- Settings saved
- Item created
- Operation completed
- Data synced

### Error Toast

**Color**: Red
**Icon**: X circle
**Use**: Alert users to failures

```
┌─────────────────────────────────────────────────────┐
│  ⊗   Failed to save file                      ×   │
│      Network connection error                      │
│      [Retry]                                        │
│  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━          │
└─────────────────────────────────────────────────────┘
```

**Example Uses**:
- Upload failed
- Save error
- Network timeout
- Permission denied
- Validation errors

### Warning Toast

**Color**: Yellow
**Icon**: Alert triangle
**Use**: Draw attention to important info

```
┌─────────────────────────────────────────────────────┐
│  ⚠   Indexing paused                          ×   │
│      Resume to keep lattice up to date               │
│      [Resume]                                       │
│  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━          │
└─────────────────────────────────────────────────────┘
```

**Example Uses**:
- Indexing paused
- Low storage space
- Sync paused
- Connection unstable
- Feature deprecated

### Info Toast

**Color**: Blue
**Icon**: Info circle
**Use**: Provide neutral information

```
┌─────────────────────────────────────────────────────┐
│  ℹ   New update available                     ×   │
│      Version 1.2.0 is ready to install             │
│      [Update Now]                                   │
│  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━          │
└─────────────────────────────────────────────────────┘
```

**Example Uses**:
- Update available
- Feature tips
- Status changes
- Informational messages
- Process complete

## Toast Positions

### Top Positions

```
Top Left           Top Center          Top Right
┌───┐                 ┌───┐                 ┌───┐
│ T │                 │ T │                 │ T │
└───┘                 └───┘                 └───┘
```

### Bottom Positions

```
Bottom Left        Bottom Center       Bottom Right
┌───┐                 ┌───┐                 ┌───┐
│ B │                 │ B │                 │ B │
└───┘                 └───┘                 └───┘
```

**Default**: Top Right

## Toast States

### Entering

```
Animation: Slide in from right (or appropriate direction)
Duration: 300ms
Easing: ease-out

  ━━━━━━━━━━━━━━━━━━━━━━━━━▶
  ┌─────────────────────┐
  │  ✓  Toast entering  │
  └─────────────────────┘
```

### Visible (Default)

```
State: Fully visible, progress bar animating
Duration: Based on configuration (default 4000ms)

┌───────────────────────────┐
│  ✓  Toast visible      × │
│  ━━━━━━━━━━━━━          │ ← Progress bar decreasing
└───────────────────────────┘
```

### Hover (Paused)

```
State: Progress bar paused, hover effect
Duration: While mouse hovers

┌───────────────────────────┐
│  ✓  Toast hovered      × │ ← Slightly elevated
│  ━━━━━━━━━━━━━          │ ← Progress bar paused
└───────────────────────────┘
```

### Exiting

```
Animation: Slide out to right (or appropriate direction)
Duration: 300ms
Easing: ease-in

┌─────────────────────┐
│  ✓  Toast exiting   │
└─────────────────────┘
━━━━━━━━━━━━━━━━━━━━━━━━━▶
```

## Multiple Toasts

### Stack (Top Right)

```
┌────────────────────────┐  ← Newest
│  ✓  Third toast     × │
│  ━━━━━━━━━━━━━━━━     │
└────────────────────────┘
     ↓ 12px gap
┌────────────────────────┐
│  ⚠  Second toast    × │
│  ━━━━━━━━━━━━━━       │
└────────────────────────┘
     ↓ 12px gap
┌────────────────────────┐  ← Oldest
│  ℹ  First toast     × │
│  ━━━━━━━━━━━━         │
└────────────────────────┘
```

**Notes**:
- Newest toasts appear at top
- Maximum 5 toasts (configurable)
- 12px gap between toasts
- Smooth transitions when adding/removing

## Responsive Behavior

### Desktop (>768px)

```
Width: max-w-sm (384px)
Position: Fixed to configured position
Shadow: Large drop shadow
```

### Mobile (<768px)

```
Width: calc(100vw - 32px) (full width with padding)
Position: Fixed to configured position
Shadow: Medium drop shadow
```

## Dark Mode

### Success (Dark)

```
┌─────────────────────────────────────────────────────┐
│  ✓   File uploaded successfully              ×   │  ← Light green text
│  Background: Dark green with low opacity           │  ← Dark bg, green tint
│  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━          │
└─────────────────────────────────────────────────────┘
```

### Error (Dark)

```
┌─────────────────────────────────────────────────────┐
│  ⊗   Upload failed                            ×   │  ← Light red text
│  Background: Dark red with low opacity             │  ← Dark bg, red tint
│  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━          │
└─────────────────────────────────────────────────────┘
```

**Color Adjustments**:
- Background: Lower opacity overlays
- Text: Lighter shades for contrast
- Icons: Brighter colors
- Borders: Darker variants
- Progress bars: Same as icon color

## Animation Details

### Enter Animation

```
Keyframes:
  0%   → opacity: 0, translateX(100%)
  100% → opacity: 1, translateX(0)

Duration: 300ms
Easing: ease-out
```

### Exit Animation

```
Keyframes:
  0%   → opacity: 1, translateX(0), scale(1)
  100% → opacity: 0, translateX(100%), scale(0.95)

Duration: 300ms
Easing: ease-in
```

### Progress Bar

```
Animation: Linear width transition
Duration: Matches toast duration
Updates: ~60fps (every 16ms)
Pauses: On hover (if pauseOnHover enabled)
```

## Interaction States

### Close Button

```
Default:  [×]  ← Gray
Hover:    [×]  ← Darker, slight background
Active:   [×]  ← Even darker, pressed effect
```

### Action Button

```
Default:  [Undo]  ← Colored text
Hover:    [Undo]  ← Underlined
Active:   [Undo]  ← Slightly darker
```

## Size Specifications

### Dimensions

```
Width:     384px (max-w-sm)
Min-Height: Auto (content-based)
Max-Height: 200px (with scroll if needed)
Padding:   16px (p-4)
Border-radius: 8px (rounded-lg)
Border-left: 4px (type color)
```

### Icon

```
Size: 20×20px (w-5 h-5)
Background: 40×40px circle (w-10 h-10)
```

### Close Button

```
Size: 16×16px icon
Clickable area: 24×24px (p-1)
```

### Progress Bar

```
Height: 4px (h-1)
Width: 0-100% (animated)
Position: Bottom edge
```

## Accessibility Indicators

### Screen Reader

```
Container:
  role="region"
  aria-label="Notifications"
  aria-live="polite"

Toast:
  role="status"
  aria-atomic="true"

Button:
  aria-label="Dismiss notification"

Progress:
  role="progressbar"
  aria-valuenow={progress}
  aria-valuemin="0"
  aria-valuemax="100"
```

### Focus Indicators

```
Default:  No outline
Focus:    2px solid blue outline, 2px offset
```

## Color Palette

### Light Mode

```
Success:
  Background: #d1fae5 (green-100)
  Border:     #10b981 (green-500)
  Text:       #065f46 (green-900)
  Icon:       #10b981 (green-500)

Error:
  Background: #fee2e2 (red-100)
  Border:     #ef4444 (red-500)
  Text:       #7f1d1d (red-900)
  Icon:       #ef4444 (red-500)

Warning:
  Background: #fef3c7 (yellow-100)
  Border:     #f59e0b (yellow-500)
  Text:       #78350f (yellow-900)
  Icon:       #f59e0b (yellow-500)

Info:
  Background: #dbeafe (blue-100)
  Border:     #3b82f6 (blue-500)
  Text:       #1e3a8a (blue-900)
  Icon:       #3b82f6 (blue-500)
```

### Dark Mode

```
Success:
  Background: rgba(16, 185, 129, 0.15)
  Border:     #34d399 (green-400)
  Text:       #d1fae5 (green-100)
  Icon:       #34d399 (green-400)

Error:
  Background: rgba(239, 68, 68, 0.15)
  Border:     #f87171 (red-400)
  Text:       #fee2e2 (red-100)
  Icon:       #f87171 (red-400)

Warning:
  Background: rgba(245, 158, 11, 0.15)
  Border:     #fbbf24 (yellow-400)
  Text:       #fef3c7 (yellow-100)
  Icon:       #fbbf24 (yellow-400)

Info:
  Background: rgba(59, 130, 246, 0.15)
  Border:     #60a5fa (blue-400)
  Text:       #dbeafe (blue-100)
  Icon:       #60a5fa (blue-400)
```

## Z-Index Hierarchy

```
Modal/Dialog:     10000
Toast Container:   9999
Dropdown:          9998
Header:            1000
Content:              1
```

## Timing Guidelines

### Duration Recommendations

```
Quick action (copy, etc):     2000ms (2s)
Standard action:               4000ms (4s) ← Default
Important action:              5000ms (5s)
Error requiring attention:     6000ms (6s)
Critical/persistent info:      0ms (never auto-dismiss)
```

### Animation Timing

```
Enter:     300ms
Exit:      300ms
Progress:  60fps updates (every ~16ms)
Hover:     150ms (opacity/scale changes)
```

## Print Styles

```
Toasts are hidden in print:
@media print {
  .toast-container { display: none; }
}
```

## Reduced Motion

```
When prefers-reduced-motion is enabled:
- No slide animations
- Instant appear/disappear
- Progress bar still visible but no animation
```

## Example Compositions

### Simple Success

```
┌────────────────────────────┐
│  ✓  Saved successfully  × │
│  ━━━━━━━━━━━━━━━━━━━     │
└────────────────────────────┘
```

### Error with Details

```
┌──────────────────────────────────────┐
│  ⊗  Upload failed                 × │
│     Network connection error         │
│  ━━━━━━━━━━━━━━━━━━━━━━━           │
└──────────────────────────────────────┘
```

### Success with Undo

```
┌──────────────────────────────────────┐
│  ✓  File deleted                  × │
│     [Undo]                           │
│  ━━━━━━━━━━━━━━━━━━━━━━━           │
└──────────────────────────────────────┘
```

### Info with Action

```
┌────────────────────────────────────────┐
│  ℹ  Update available               × │
│     Version 1.2.0 ready to install    │
│     [Update Now]                       │
│  ━━━━━━━━━━━━━━━━━━━━━━━━━━━        │
└────────────────────────────────────────┘
```

This visual reference helps designers and developers understand the exact appearance and behavior of the toast notification system.
