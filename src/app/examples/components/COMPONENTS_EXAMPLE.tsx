/**
 * UI Primitives Usage Examples
 *
 * This file demonstrates how to use the Tooltip and DatePicker components
 * in various scenarios throughout the Vault desktop app.
 */

import { useState } from 'react';
import { Tooltip, TooltipProvider, DatePicker } from './index';
import { Trash2, Download, Calendar, ExternalLink, Settings } from 'lucide-react';

/**
 * Example 1: Basic Tooltip Usage
 * Icon buttons with helpful tooltips
 */
export function ToolbarWithTooltips() {
  return (
    <div className="flex gap-2 p-4 bg-[var(--surface-elevated)] rounded-lg">
      <Tooltip content="Delete item" side="bottom">
        <button className="p-2 hover:bg-[var(--surface-hover)] rounded">
          <Trash2 className="h-5 w-5" />
        </button>
      </Tooltip>

      <Tooltip content="Download file" side="bottom">
        <button className="p-2 hover:bg-[var(--surface-hover)] rounded">
          <Download className="h-5 w-5" />
        </button>
      </Tooltip>

      <Tooltip content="Open in external app" side="bottom">
        <button className="p-2 hover:bg-[var(--surface-hover)] rounded">
          <ExternalLink className="h-5 w-5" />
        </button>
      </Tooltip>

      <Tooltip content="Settings" side="bottom">
        <button className="p-2 hover:bg-[var(--surface-hover)] rounded">
          <Settings className="h-5 w-5" />
        </button>
      </Tooltip>
    </div>
  );
}

/**
 * Example 2: DatePicker for Search Filters
 * Date range selection for filtering search results
 */
export function SearchDateFilters() {
  const [startDate, setStartDate] = useState<Date>();
  const [endDate, setEndDate] = useState<Date>();

  return (
    <div className="space-y-4 p-4 bg-[var(--surface-elevated)] rounded-lg">
      <h3 className="text-lg font-semibold">Filter by Date Range</h3>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <label className="block text-sm font-medium mb-2">From Date</label>
          <DatePicker
            selected={startDate}
            onSelect={setStartDate}
            maxDate={endDate} // Can't select after end date
            placeholder="Start date"
            className="w-full"
          />
        </div>

        <div>
          <label className="block text-sm font-medium mb-2">To Date</label>
          <DatePicker
            selected={endDate}
            onSelect={setEndDate}
            minDate={startDate} // Can't select before start date
            placeholder="End date"
            className="w-full"
          />
        </div>
      </div>

      {startDate && endDate && (
        <div className="text-sm text-[var(--text-secondary)]">
          Showing results from {startDate.toLocaleDateString()} to {endDate.toLocaleDateString()}
        </div>
      )}
    </div>
  );
}

/**
 * Example 3: Rich Tooltip Content
 * Tooltips can contain more than just text
 */
export function RichTooltipExample() {
  return (
    <div className="flex gap-4 p-4">
      <Tooltip
        content={
          <div className="space-y-1">
            <p className="font-semibold text-white">Keyboard Shortcut</p>
            <p className="text-sm text-gray-200">Press Cmd+K to open</p>
          </div>
        }
        side="right"
      >
        <button className="px-4 py-2 bg-[var(--accent-primary)] text-white rounded-lg hover:bg-[var(--accent-hover)]">
          Quick Search
        </button>
      </Tooltip>

      <Tooltip
        content={
          <div className="max-w-xs">
            <p className="font-medium mb-1">Advanced Mode</p>
            <p className="text-sm">
              Enable advanced features including semantic search,
              custom filters, and query rewriting.
            </p>
          </div>
        }
        side="bottom"
      >
        <button className="px-4 py-2 border border-[var(--border-color)] rounded-lg hover:bg-[var(--bg-secondary)]">
          Enable Advanced Mode
        </button>
      </Tooltip>
    </div>
  );
}

/**
 * Example 4: Conditional Tooltips
 * Show tooltips based on state
 */
export function ConditionalTooltipExample() {
  const [isProcessing, setIsProcessing] = useState(false);

  return (
    <div className="p-4">
      <Tooltip
        content={isProcessing ? 'Processing...' : 'Click to start processing'}
        disabled={isProcessing} // Disable tooltip when processing
        side="top"
      >
        <button
          onClick={() => setIsProcessing(!isProcessing)}
          disabled={isProcessing}
          className="px-4 py-2 bg-[var(--success)] text-white rounded-lg hover:bg-[var(--success)] disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {isProcessing ? 'Processing...' : 'Process Documents'}
        </button>
      </Tooltip>
    </div>
  );
}

/**
 * Example 5: Event Date Picker
 * DatePicker with constraints for future dates only
 */
export function EventDatePicker() {
  const [eventDate, setEventDate] = useState<Date>();
  const today = new Date();

  return (
    <div className="space-y-3 p-4 bg-[var(--surface-elevated)] rounded-lg">
      <label className="block text-sm font-medium">
        Select Event Date
      </label>
      <DatePicker
        selected={eventDate}
        onSelect={setEventDate}
        minDate={today} // Only future dates
        placeholder="Choose a date"
        dateFormat="MMMM d, yyyy"
      />
      {eventDate && (
        <p className="text-sm text-[var(--text-secondary)]">
          Event scheduled for: {eventDate.toLocaleDateString('en-US', {
            weekday: 'long',
            year: 'numeric',
            month: 'long',
            day: 'numeric',
          })}
        </p>
      )}
    </div>
  );
}

/**
 * Example 6: Complete UI with Both Components
 * Real-world scenario combining tooltips and date pickers
 */
export function DocumentFilterPanel() {
  const [createdAfter, setCreatedAfter] = useState<Date>();
  const [createdBefore, setCreatedBefore] = useState<Date>();

  const handleClearFilters = () => {
    setCreatedAfter(undefined);
    setCreatedBefore(undefined);
  };

  return (
    <div className="p-6 bg-[var(--surface-elevated)] rounded-lg shadow-sm border border-[var(--border-color)]">
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-lg font-semibold">Document Filters</h3>
        <Tooltip content="Clear all filters" side="left">
          <button
            onClick={handleClearFilters}
            className="text-sm text-[var(--accent-primary)] hover:text-[var(--accent-primary)]"
          >
            Clear
          </button>
        </Tooltip>
      </div>

      <div className="space-y-4">
        <div>
          <label className="block text-sm font-medium mb-2">
            Created After
          </label>
          <DatePicker
            selected={createdAfter}
            onSelect={setCreatedAfter}
            maxDate={createdBefore}
            placeholder="Select date"
          />
        </div>

        <div>
          <label className="block text-sm font-medium mb-2">
            Created Before
          </label>
          <DatePicker
            selected={createdBefore}
            onSelect={setCreatedBefore}
            minDate={createdAfter}
            placeholder="Select date"
          />
        </div>
      </div>

      <div className="mt-4 flex gap-2">
        <Tooltip content="Apply current filters to search" side="top">
          <button className="flex-1 px-4 py-2 bg-[var(--accent-primary)] text-white rounded-lg hover:bg-[var(--accent-hover)]">
            Apply Filters
          </button>
        </Tooltip>

        <Tooltip content="Save these filters for later" side="top">
          <button className="px-4 py-2 border border-[var(--border-color)] rounded-lg hover:bg-[var(--bg-secondary)]">
            <Calendar className="h-5 w-5" />
          </button>
        </Tooltip>
      </div>
    </div>
  );
}

/**
 * Main Example Component
 * Wrap all examples in TooltipProvider
 */
export function UIComponentsShowcase() {
  return (
    <TooltipProvider delayDuration={200}>
      <div className="space-y-8 p-8">
        <section>
          <h2 className="text-2xl font-bold mb-4">Tooltip Examples</h2>
          <div className="space-y-4">
            <ToolbarWithTooltips />
            <RichTooltipExample />
            <ConditionalTooltipExample />
          </div>
        </section>

        <section>
          <h2 className="text-2xl font-bold mb-4">DatePicker Examples</h2>
          <div className="grid grid-cols-2 gap-4">
            <SearchDateFilters />
            <EventDatePicker />
          </div>
        </section>

        <section>
          <h2 className="text-2xl font-bold mb-4">Combined Example</h2>
          <DocumentFilterPanel />
        </section>
      </div>
    </TooltipProvider>
  );
}
