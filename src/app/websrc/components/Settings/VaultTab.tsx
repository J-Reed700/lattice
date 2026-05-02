/**
 * Vault Settings Tab
 *
 * Surfaces the markdown-mirror feature: pick a folder on disk, flip
 * "Enabled" to start writing notes there, optionally watch for external
 * edits. Backend is the SSOT (Rust SettingsRepository) — this tab uses
 * `useSettingsQuery` for reads and `useUpdateSettingsMutation` for writes.
 *
 * The first time the user flips Enabled to true, the backend does a
 * one-shot backfill of every existing note into `<vault>/notes/<id>.md`
 * with YAML frontmatter. Subsequent saves write through automatically.
 *
 * Safe-by-default:
 * - Disabled by default — explicit opt-in.
 * - Empty vaultPath uses the default `~/Lattice` so users don't have
 *   to think about the layout to get started.
 * - Path validation runs in the Rust use case before any write lands.
 */

import { useState } from 'react';

import { open } from '@tauri-apps/plugin-dialog';
import { Folder, FolderOpen, Info } from 'lucide-react';

import {
  useSettingsQuery,
  useUpdateSettingsMutation,
} from '../../hooks/queries/useSettingsQuery';
import { toast } from '../../stores/toastStore';

export function VaultTab() {
  const { data: settings, isLoading } = useSettingsQuery();
  const updateSettingsMutation = useUpdateSettingsMutation();
  const [isPicking, setIsPicking] = useState(false);

  const vault = settings?.vault;
  const vaultPath = vault?.vaultPath ?? '';
  const enabled = vault?.enabled ?? false;
  const watchExternal = vault?.watchExternalChanges ?? false;

  const isSyncing = isLoading;
  const isSaving = updateSettingsMutation.isPending;

  const handlePickFolder = async () => {
    setIsPicking(true);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Vault Folder',
      });
      if (typeof selected === 'string' && selected.trim()) {
        updateSettingsMutation.mutate(
          { category: 'vault', updates: { vaultPath: selected } },
          {
            onError: (error) => {
              toast.error("Couldn't save vault folder", { message: error.message });
            },
            onSuccess: () => {
              toast.success('Vault folder updated');
            },
          },
        );
      }
    } finally {
      setIsPicking(false);
    }
  };

  const handleResetToDefault = () => {
    updateSettingsMutation.mutate(
      { category: 'vault', updates: { vaultPath: '' } },
      {
        onError: (error) => {
          toast.error("Couldn't reset vault folder", { message: error.message });
        },
        onSuccess: () => {
          toast.success('Vault folder reset to default (~/Lattice)');
        },
      },
    );
  };

  const handleEnabledChange = (next: boolean) => {
    updateSettingsMutation.mutate(
      { category: 'vault', updates: { enabled: next } },
      {
        onError: (error) => {
          toast.error("Couldn't toggle vault export", { message: error.message });
        },
        onSuccess: () => {
          toast.success(
            next ? 'Vault export enabled — backfilling existing notes' : 'Vault export disabled',
          );
        },
      },
    );
  };

  const handleWatchChange = (next: boolean) => {
    updateSettingsMutation.mutate(
      { category: 'vault', updates: { watchExternalChanges: next } },
      {
        onError: (error) => {
          toast.error("Couldn't toggle external watcher", { message: error.message });
        },
      },
    );
  };

  return (
    <div className="space-y-6">
      <header>
        <h2 className="text-lg font-semibold text-[hsl(var(--text-primary))]">
          Vault
        </h2>
        <p className="text-sm text-[hsl(var(--text-secondary))] mt-1">
          Mirror your notes as plain markdown files on disk so you can manage them with
          Obsidian, git, iCloud, or anything else. SQLite stays the index; the vault
          folder is your portable copy.
        </p>
      </header>

      {/* Vault folder */}
      <section className="space-y-2">
        <label className="block text-sm font-medium text-[hsl(var(--text-primary))]">
          Vault folder
        </label>
        <div className="flex items-center gap-2">
          <div className="flex-1 min-w-0 px-3 py-2 rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] text-sm text-[hsl(var(--text-primary))] truncate">
            <span className="inline-flex items-center gap-2">
              <Folder className="w-4 h-4 text-[hsl(var(--text-tertiary))] shrink-0" />
              <span className="truncate">
                {vaultPath || (
                  <span className="text-[hsl(var(--text-tertiary))]">
                    Default: ~/Lattice
                  </span>
                )}
              </span>
            </span>
          </div>
          <button
            type="button"
            onClick={handlePickFolder}
            disabled={isPicking || isSaving || isSyncing}
            className="inline-flex items-center gap-1.5 px-3 py-2 rounded-md border border-[hsl(var(--border-default))] bg-[hsl(var(--surface))] text-sm text-[hsl(var(--text-primary))] hover:bg-[hsl(var(--surface-raised))] disabled:opacity-50"
          >
            <FolderOpen className="w-4 h-4" />
            Choose…
          </button>
          {vaultPath && (
            <button
              type="button"
              onClick={handleResetToDefault}
              disabled={isSaving || isSyncing}
              className="px-3 py-2 rounded-md text-sm text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] disabled:opacity-50"
            >
              Reset
            </button>
          )}
        </div>
        <p className="text-xs text-[hsl(var(--text-tertiary))]">
          Notes will be written to <code>{vaultPath || '~/Lattice'}/notes/&lt;id&gt;.md</code>{' '}
          with YAML frontmatter.
        </p>
      </section>

      {/* Enable toggle */}
      <Toggle
        title="Mirror notes to disk"
        description="When on, every note save also writes a markdown file to the vault folder. Turning it on the first time backfills your existing notes."
        checked={enabled}
        disabled={isSaving || isSyncing}
        onChange={handleEnabledChange}
      />

      {/* External-watcher toggle (placeholder UX — implementation lands in a follow-up) */}
      <Toggle
        title="Watch for external edits"
        description="Re-import changes you make in Obsidian / VS Code / etc. back into Lattice. Off until v1.1 — for now, edits made outside Lattice won't sync back."
        checked={watchExternal}
        disabled={true}
        onChange={handleWatchChange}
      />

      {/* Footnote */}
      <div className="flex items-start gap-2 text-xs text-[hsl(var(--text-tertiary))] pt-4 border-t border-[hsl(var(--border-subtle))]">
        <Info className="w-3.5 h-3.5 mt-0.5 shrink-0" />
        <p>
          Frontmatter format is Obsidian-compatible. Files are written atomically
          (temp file + rename) so an external watcher never sees a half-written file.
        </p>
      </div>
    </div>
  );
}

interface ToggleProps {
  title: string;
  description: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (next: boolean) => void;
}

function Toggle({ title, description, checked, disabled, onChange }: ToggleProps) {
  return (
    <div className="flex items-start justify-between gap-4 py-2">
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-[hsl(var(--text-primary))]">{title}</p>
        <p className="text-xs text-[hsl(var(--text-secondary))] mt-1">{description}</p>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={`relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors ${
          checked ? 'bg-[hsl(var(--accent))]' : 'bg-[hsl(var(--border-default))]'
        } disabled:opacity-50 disabled:cursor-not-allowed`}
      >
        <span
          className={`inline-block h-3.5 w-3.5 rounded-full bg-white transform transition-transform ${
            checked ? 'translate-x-5' : 'translate-x-1'
          }`}
        />
      </button>
    </div>
  );
}
