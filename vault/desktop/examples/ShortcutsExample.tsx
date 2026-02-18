/**
 * Keyboard Shortcuts Usage Examples
 *
 * This file demonstrates how to use the keyboard shortcuts system
 * in different components and scenarios.
 */

import { useState } from 'react';
import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts';

// ========================================
// Example 1: Basic Shortcut Registration
// ========================================

export function BasicExample() {
  const [count, setCount] = useState(0);

  useKeyboardShortcuts([
    {
      id: 'example.increment',
      keys: 'Mod+ArrowUp',
      description: 'Increment counter',
      category: 'global',
      handler: () => setCount(c => c + 1),
    },
    {
      id: 'example.decrement',
      keys: 'Mod+ArrowDown',
      description: 'Decrement counter',
      category: 'global',
      handler: () => setCount(c => c - 1),
    },
    {
      id: 'example.reset',
      keys: 'Mod+0',
      description: 'Reset counter',
      category: 'global',
      handler: () => setCount(0),
    },
  ]);

  return (
    <div>
      <h2>Counter: {count}</h2>
      <p>Press ⌘↑ to increment, ⌘↓ to decrement, ⌘0 to reset</p>
    </div>
  );
}

// ========================================
// Example 2: Context-Aware Shortcuts
// ========================================

export function ContextAwareExample() {
  const [mode, setMode] = useState<'read' | 'edit'>('read');
  const [content, setContent] = useState('');

  // Read mode shortcuts
  useKeyboardShortcuts(
    [
      {
        id: 'example.enterEdit',
        keys: 'Mod+E',
        description: 'Enter edit mode',
        category: 'editor',
        handler: () => setMode('edit'),
      },
    ],
    { context: 'search', enabled: mode === 'read' }
  );

  // Edit mode shortcuts
  useKeyboardShortcuts(
    [
      {
        id: 'example.save',
        keys: 'Mod+S',
        description: 'Save and exit edit mode',
        category: 'editor',
        handler: () => {
          console.log('Saving:', content);
          setMode('read');
        },
      },
      {
        id: 'example.cancel',
        keys: 'Escape',
        description: 'Cancel edit',
        category: 'editor',
        handler: () => setMode('read'),
      },
    ],
    { context: 'editor', enabled: mode === 'edit' }
  );

  return (
    <div>
      <h2>Mode: {mode}</h2>
      {mode === 'read' ? (
        <div>
          <p>{content || 'No content yet'}</p>
          <p>Press ⌘E to edit</p>
        </div>
      ) : (
        <div>
          <textarea
            value={content}
            onChange={e => setContent(e.target.value)}
            autoFocus
          />
          <p>Press ⌘S to save, Esc to cancel</p>
        </div>
      )}
    </div>
  );
}

// ========================================
// Example 3: List Navigation
// ========================================

export function ListNavigationExample() {
  const items = ['Item 1', 'Item 2', 'Item 3', 'Item 4', 'Item 5'];
  const [selectedIndex, setSelectedIndex] = useState(0);

  useKeyboardShortcuts([
    {
      id: 'example.nextItem',
      keys: 'ArrowDown',
      description: 'Select next item',
      category: 'navigation',
      handler: () => setSelectedIndex(i => Math.min(i + 1, items.length - 1)),
    },
    {
      id: 'example.prevItem',
      keys: 'ArrowUp',
      description: 'Select previous item',
      category: 'navigation',
      handler: () => setSelectedIndex(i => Math.max(i - 1, 0)),
    },
    {
      id: 'example.selectFirst',
      keys: 'Home',
      description: 'Select first item',
      category: 'navigation',
      handler: () => setSelectedIndex(0),
    },
    {
      id: 'example.selectLast',
      keys: 'End',
      description: 'Select last item',
      category: 'navigation',
      handler: () => setSelectedIndex(items.length - 1),
    },
    {
      id: 'example.quickSelect1',
      keys: '1',
      description: 'Select first item',
      category: 'navigation',
      handler: () => setSelectedIndex(0),
    },
    {
      id: 'example.quickSelect2',
      keys: '2',
      description: 'Select second item',
      category: 'navigation',
      handler: () => setSelectedIndex(1),
    },
    {
      id: 'example.quickSelect3',
      keys: '3',
      description: 'Select third item',
      category: 'navigation',
      handler: () => setSelectedIndex(2),
    },
  ]);

  return (
    <div>
      <h2>List Navigation</h2>
      <ul>
        {items.map((item, index) => (
          <li
            key={index}
            style={{
              fontWeight: index === selectedIndex ? 'bold' : 'normal',
              backgroundColor: index === selectedIndex ? '#e0e0e0' : 'transparent',
            }}
          >
            {item}
          </li>
        ))}
      </ul>
      <p>Use ↑/↓ to navigate, Home/End to jump, 1-3 for quick select</p>
    </div>
  );
}

// ========================================
// Example 4: Global vs Local Shortcuts
// ========================================

export function GlobalVsLocalExample() {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [logs, setLogs] = useState<string[]>([]);

  const addLog = (message: string) => {
    setLogs(prev => [...prev, `${new Date().toLocaleTimeString()}: ${message}`]);
  };

  // Global shortcut - works everywhere
  useKeyboardShortcuts([
    {
      id: 'example.globalSearch',
      keys: 'Mod+K',
      description: 'Open search (global)',
      category: 'global',
      handler: () => addLog('Global search triggered'),
      global: true, // Works even in inputs
    },
    {
      id: 'example.openDialog',
      keys: 'Mod+O',
      description: 'Open dialog',
      category: 'global',
      handler: () => setDialogOpen(true),
    },
  ]);

  // Dialog-specific shortcuts
  useKeyboardShortcuts(
    [
      {
        id: 'example.closeDialog',
        keys: 'Escape',
        description: 'Close dialog',
        category: 'global',
        handler: () => setDialogOpen(false),
      },
      {
        id: 'example.submitDialog',
        keys: 'Mod+Enter',
        description: 'Submit dialog',
        category: 'global',
        handler: () => {
          addLog('Dialog submitted');
          setDialogOpen(false);
        },
      },
    ],
    { enabled: dialogOpen }
  );

  return (
    <div>
      <h2>Global vs Local Shortcuts</h2>
      <div>
        <p>Press ⌘K anywhere (even in input) - global shortcut</p>
        <p>Press ⌘O to open dialog - local shortcut</p>
        <input type="text" placeholder="Type here and try ⌘K" />
      </div>

      {dialogOpen && (
        <div style={{ border: '1px solid black', padding: '20px', marginTop: '20px' }}>
          <h3>Dialog</h3>
          <p>Press Esc to close, ⌘Enter to submit</p>
          <button onClick={() => setDialogOpen(false)}>Close</button>
        </div>
      )}

      <div style={{ marginTop: '20px' }}>
        <h3>Event Log</h3>
        <ul>
          {logs.map((log, i) => (
            <li key={i}>{log}</li>
          ))}
        </ul>
      </div>
    </div>
  );
}

// ========================================
// Example 5: Conflict Detection
// ========================================

export function ConflictDetectionExample() {
  const [message, setMessage] = useState('');

  const { conflicts } = useKeyboardShortcuts([
    {
      id: 'example.conflict1',
      keys: 'Mod+J',
      description: 'First action',
      category: 'global',
      handler: () => setMessage('First handler called'),
    },
    {
      id: 'example.conflict2',
      keys: 'Mod+J', // Same shortcut!
      description: 'Second action',
      category: 'global',
      handler: () => setMessage('Second handler called'),
    },
  ]);

  return (
    <div>
      <h2>Conflict Detection</h2>
      {conflicts.length > 0 && (
        <div style={{ color: 'red', marginBottom: '10px' }}>
          <strong>Conflicts detected:</strong>
          <ul>
            {conflicts.map((conflict, i) => (
              <li key={i}>
                {conflict.shortcut1} and {conflict.shortcut2} both use {conflict.keys}
              </li>
            ))}
          </ul>
        </div>
      )}
      <p>Try pressing ⌘J</p>
      <p>Message: {message}</p>
    </div>
  );
}

// ========================================
// Example 6: Customizable Shortcuts
// ========================================

export function CustomizableExample() {
  const [logs, setLogs] = useState<string[]>([]);

  const addLog = (message: string) => {
    setLogs(prev => [...prev, message]);
  };

  const { updateShortcut, resetShortcut } = useKeyboardShortcuts([
    {
      id: 'example.customizable',
      keys: 'Mod+J',
      description: 'Customizable action',
      category: 'global',
      handler: () => addLog('Customizable shortcut triggered!'),
    },
  ]);

  return (
    <div>
      <h2>Customizable Shortcuts</h2>
      <p>Current shortcut: ⌘J</p>
      <button onClick={() => updateShortcut('example.customizable', 'Mod+Shift+J')}>
        Change to ⌘⇧J
      </button>
      <button onClick={() => resetShortcut('example.customizable')}>
        Reset to Default
      </button>

      <div style={{ marginTop: '20px' }}>
        <h3>Trigger Log</h3>
        <ul>
          {logs.map((log, i) => (
            <li key={i}>{log}</li>
          ))}
        </ul>
      </div>
    </div>
  );
}

// ========================================
// Example 7: Search with Number Shortcuts
// ========================================

export function SearchResultsExample() {
  const results = [
    'First result',
    'Second result',
    'Third result',
    'Fourth result',
    'Fifth result',
  ];
  const [selected, setSelected] = useState<number | null>(null);

  useKeyboardShortcuts([
    {
      id: 'example.selectResult1',
      keys: '1',
      description: 'Select first result',
      category: 'search',
      handler: () => setSelected(0),
    },
    {
      id: 'example.selectResult2',
      keys: '2',
      description: 'Select second result',
      category: 'search',
      handler: () => setSelected(1),
    },
    {
      id: 'example.selectResult3',
      keys: '3',
      description: 'Select third result',
      category: 'search',
      handler: () => setSelected(2),
    },
    {
      id: 'example.openSelected',
      keys: 'Enter',
      description: 'Open selected result',
      category: 'search',
      handler: () => {
        if (selected !== null) {
          alert(`Opening: ${results[selected]}`);
        }
      },
    },
  ], { context: 'search' });

  return (
    <div>
      <h2>Search Results</h2>
      <p>Press 1-3 to select, Enter to open</p>
      <ul>
        {results.map((result, index) => (
          <li
            key={index}
            style={{
              fontWeight: index === selected ? 'bold' : 'normal',
              backgroundColor: index === selected ? '#e0e0e0' : 'transparent',
            }}
          >
            {result}
          </li>
        ))}
      </ul>
    </div>
  );
}
