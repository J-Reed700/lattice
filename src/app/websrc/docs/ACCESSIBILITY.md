# Accessibility Guidelines

## Overview

This document outlines the accessibility (a11y) standards and best practices implemented throughout the Lattice/Lattice application. We strive to meet WCAG 2.1 Level AA standards to ensure our application is usable by everyone.

## Table of Contents

1. [Keyboard Navigation](#keyboard-navigation)
2. [Screen Reader Support](#screen-reader-support)
3. [Visual Accessibility](#visual-accessibility)
4. [Component-Specific Guidelines](#component-specific-guidelines)
5. [Testing Checklist](#testing-checklist)
6. [Known Issues](#known-issues)

---

## Keyboard Navigation

### Global Shortcuts

All interactive elements are keyboard accessible using standard patterns:

- **Tab**: Navigate forward through interactive elements
- **Shift + Tab**: Navigate backward through interactive elements
- **Enter/Space**: Activate buttons and links
- **Escape**: Close dialogs, modals, and dropdowns
- **Arrow Keys**: Navigate within lists, menus, and select elements

### Focus Management

- **Focus Visible**: All interactive elements show a visible focus indicator (blue ring)
- **Focus Trap**: Dialogs and modals trap focus within themselves
- **Focus Restoration**: Focus returns to triggering element when closing dialogs
- **Skip Links**: Future enhancement for quick navigation to main content

### Component Keyboard Support

#### Button
- `Space` or `Enter` - Activate button
- Focus indicator on keyboard navigation
- Disabled buttons are not focusable

#### Input
- Standard text input keyboard navigation
- `Escape` - Clear input (if clear button is enabled)
- `Tab` - Move to next field
- Auto-focus on validation errors

#### Dialog
- `Escape` - Close dialog
- `Tab` - Cycle through dialog elements (focus trapped)
- Focus moves to dialog on open
- Focus returns to trigger on close

#### Search
- `Cmd/Ctrl + K` - Open command palette (future)
- `Cmd/Ctrl + R` - Show query rewrites
- `Enter` - Execute search
- `Arrow Up/Down` - Navigate results (future)

---

## Screen Reader Support

### ARIA Labels and Roles

All components use appropriate ARIA attributes:

#### Semantic HTML First
- Use native HTML elements whenever possible (`<button>`, `<input>`, etc.)
- Only use ARIA when native semantics are insufficient

#### ARIA Attributes Used

**Common Patterns:**
- `role="dialog"` - Modal dialogs
- `role="alert"` - Error messages
- `role="status"` - Loading indicators and toast notifications
- `role="list"` / `role="listitem"` - Custom lists
- `aria-label` - Accessible labels for icon buttons
- `aria-labelledby` - Associate labels with elements
- `aria-describedby` - Associate descriptions with elements
- `aria-live="polite"` - Announce dynamic content
- `aria-invalid` - Mark invalid form fields
- `aria-expanded` - Indicate expandable state
- `aria-hidden` - Hide decorative elements

### Screen Reader Announcements

**Dynamic Content:**
- Search results loading/loaded
- Form validation errors
- Upload progress
- Toast notifications
- Error states

**Example Implementation:**
```tsx
// Loading state with screen reader announcement
<div role="status" aria-label="Loading search results" aria-live="polite">
  <div className="animate-spin..." />
  <span className="sr-only">Loading search results...</span>
</div>
```

### Live Regions

Use `aria-live` for dynamic content:
- `aria-live="polite"` - Status updates, search results
- `aria-live="assertive"` - Critical errors
- `aria-atomic="true"` - Read entire region on change

---

## Visual Accessibility

### Color Contrast

All text and interactive elements meet WCAG AA standards:

- **Normal text**: 4.5:1 minimum contrast ratio
- **Large text** (18pt+): 3:1 minimum contrast ratio
- **Interactive elements**: 3:1 minimum contrast ratio

**Color Usage:**
- Never rely on color alone to convey information
- Use icons, labels, and patterns in addition to color
- Error states use both red color AND error icon
- Success states use both green color AND check icon

### Dark Mode

- Full dark mode support with appropriate contrast ratios
- Uses system preference by default (`prefers-color-scheme`)
- Manual toggle available
- All colors tested for contrast in both modes

### Typography

- **Base font size**: 16px (browser default)
- **Line height**: 1.5 for body text
- **Scalable units**: rem/em used for all font sizes
- **Respects user preferences**: Text scales with browser zoom

### Motion and Animation

Respects user's motion preferences:

```css
@media (prefers-reduced-motion: reduce) {
  * {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}
```

**Animated elements:**
- Loading spinners
- Button hover effects
- Dialog transitions
- Toast notifications
- Skeleton loaders

---

## Component-Specific Guidelines

### Button Component

**Accessibility Features:**
- Semantic `<button>` element
- Clear focus indicator
- Loading state with spinner and "Loading..." text
- Disabled state prevents interaction and is not focusable
- Icon buttons include `aria-label`

**Best Practices:**
```tsx
// Good - Clear label
<Button>Save Changes</Button>

// Good - Icon button with aria-label
<Button aria-label="Close dialog">
  <X className="w-4 h-4" />
</Button>

// Bad - No accessible label
<Button>
  <X className="w-4 h-4" />
</Button>
```

### Input Component

**Accessibility Features:**
- Associated `<label>` element
- Error messages announced to screen readers
- `aria-invalid` when validation fails
- `aria-describedby` links to helper/error text
- Clear focus indicator
- Character counter for limited inputs

**Best Practices:**
```tsx
// Good - Complete accessibility
<Input
  label="Email Address"
  error={errors.email}
  helperText="We'll never share your email"
  aria-required="true"
/>

// Bad - Missing label
<Input placeholder="Enter email" />
```

### Dialog Component

**Accessibility Features:**
- Focus trap within dialog
- `Escape` key to close
- Focus returns to trigger on close
- `role="dialog"` and `aria-modal="true"`
- `aria-labelledby` and `aria-describedby`
- Backdrop click to close (optional)
- Prevents body scroll when open

**Best Practices:**
```tsx
<Dialog
  open={isOpen}
  onOpenChange={setIsOpen}
  title="Confirm Action" // Used for aria-labelledby
  description="This action cannot be undone" // Used for aria-describedby
>
  <DialogContent />
</Dialog>
```

### Toast Component

**Accessibility Features:**
- `role="status"` for notifications
- `aria-live="polite"` for announcements
- Auto-dismiss with configurable duration
- Manual dismiss button with `aria-label`
- Icon + text for multi-sensory feedback

### Search Results

**Accessibility Features:**
- `role="list"` for results container
- `role="listitem"` for each result
- Keyboard navigation (future enhancement)
- Screen reader announces result count
- Click and keyboard activation

**Best Practices:**
```tsx
<div role="list" aria-label={`${results.length} search results`}>
  {results.map((result, index) => (
    <Card
      role="listitem"
      aria-label={`Result ${index + 1}: ${result.fileName}`}
      onClick={() => handleClick(result)}
    >
      {/* Result content */}
    </Card>
  ))}
</div>
```

### Form Validation

**Accessibility Features:**
- Inline validation on blur
- Error messages associated with fields
- Error icon + text for clarity
- Submit prevention when invalid
- Focus on first error field

**Best Practices:**
```tsx
const validateEmail = (value: string) => {
  if (!value) return 'Email is required';
  if (!isValidEmail(value)) return 'Please enter a valid email';
};

<Input
  label="Email"
  validateOnBlur={true}
  validate={validateEmail}
  aria-required="true"
/>
```

---

## Testing Checklist

### Manual Testing

#### Keyboard Navigation
- [ ] All interactive elements are keyboard accessible
- [ ] Tab order is logical
- [ ] Focus indicators are visible
- [ ] No keyboard traps (except intentional in dialogs)
- [ ] Shortcuts work as documented

#### Screen Reader Testing
- [ ] Test with NVDA (Windows)
- [ ] Test with JAWS (Windows)
- [ ] Test with VoiceOver (macOS)
- [ ] All images have alt text
- [ ] Form errors are announced
- [ ] Dynamic content changes are announced
- [ ] Loading states are announced

#### Visual Testing
- [ ] 200% zoom - content is readable
- [ ] Color contrast meets WCAG AA
- [ ] Dark mode maintains contrast
- [ ] Text is readable at different sizes
- [ ] Icons are not sole indicators

#### Motion Testing
- [ ] Test with `prefers-reduced-motion`
- [ ] Animations can be disabled
- [ ] No auto-playing animations

### Automated Testing

Use these tools for automated accessibility testing:

1. **axe DevTools** - Browser extension for real-time testing
2. **Lighthouse** - Built into Chrome DevTools
3. **WAVE** - WebAIM's browser extension
4. **eslint-plugin-jsx-a11y** - Linting for React components

### Quick Test Commands

```bash
# Run accessibility tests
npm run test:a11y

# Lint for accessibility issues
npm run lint:a11y

# Generate accessibility report
npm run a11y:report
```

---

## Known Issues

### Current Limitations

1. **Keyboard Navigation in Search Results**
   - Status: Planned for future release
   - Workaround: Use Tab key to navigate, Enter to activate

2. **High Contrast Mode**
   - Status: Partial support
   - Issue: Some custom borders may not appear in Windows High Contrast Mode
   - Workaround: Standard focus indicators still visible

3. **Complex Data Tables**
   - Status: Under development
   - Issue: Some data tables need better header associations
   - Workaround: Screen reader users may need to navigate cell by cell

### Reporting Issues

If you discover an accessibility issue:

1. Check if it's already listed in Known Issues
2. Open an issue on GitHub with:
   - Description of the issue
   - Steps to reproduce
   - Assistive technology used (if applicable)
   - Screenshots or recordings
   - Suggested fix (if any)

---

## Resources

### WCAG Guidelines
- [WCAG 2.1 Level AA](https://www.w3.org/WAI/WCAG21/quickref/?versions=2.1&levels=aa)
- [ARIA Authoring Practices](https://www.w3.org/WAI/ARIA/apg/)

### Testing Tools
- [axe DevTools](https://www.deque.com/axe/devtools/)
- [NVDA Screen Reader](https://www.nvaccess.org/)
- [VoiceOver User Guide](https://support.apple.com/guide/voiceover/welcome/mac)

### Learning Resources
- [WebAIM Articles](https://webaim.org/articles/)
- [A11y Project Checklist](https://www.a11yproject.com/checklist/)
- [Inclusive Components](https://inclusive-components.design/)

---

## Contributing

When adding new features or components:

1. Review this accessibility guide
2. Use semantic HTML first
3. Add appropriate ARIA attributes
4. Test with keyboard only
5. Test with a screen reader
6. Verify color contrast
7. Document any accessibility considerations

**Remember:** Accessibility is not an afterthought—it should be considered from the start of development.
