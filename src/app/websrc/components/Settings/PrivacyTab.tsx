/**
 * Privacy Settings Tab
 *
 * Configure telemetry, crash reporting, and data privacy options.
 */

import { Shield, Activity, AlertTriangle, Lock } from 'lucide-react';

import { useSettingsStore } from '../../stores/settingsStore';

export function PrivacyTab() {
  const privacySettings = useSettingsStore((state) => state.settings.privacy);
  const updatePrivacy = useSettingsStore((state) => state.updatePrivacy);

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center gap-3 pb-4 border-b border-[hsl(var(--border-subtle))]">
        <div className="p-2 bg-[hsl(var(--accent-muted))] rounded-lg">
          <Shield className="w-5 h-5 text-[hsl(var(--accent))]" />
        </div>
        <div>
          <h2 className="text-xl font-semibold text-[hsl(var(--text-primary))]">Privacy Settings</h2>
          <p className="text-sm text-[hsl(var(--text-secondary))]">
            Control what data is shared and how
          </p>
        </div>
      </div>

      {/* Privacy Philosophy */}
      <div className="p-4 bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))]">
        <div className="flex gap-3">
          <div className="flex-shrink-0">
            <Lock className="w-5 h-5 text-[hsl(var(--accent))]" />
          </div>
          <div className="flex-1">
            <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))] mb-2">
              Privacy First
            </h3>
            <p className="text-xs text-[hsl(var(--text-secondary))]">
              Lattice Lattice is designed with privacy at its core. All your documents and data are
              stored locally on your device. We never upload your content to any server.
            </p>
          </div>
        </div>
      </div>

      {/* Telemetry */}
      <div className="space-y-4">
        <div className="flex items-center gap-2">
          <Activity className="w-4 h-4 text-[hsl(var(--accent))]" />
          <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">Usage Analytics</h3>
        </div>

        <div className="p-4 bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))]">
          <div className="flex items-start gap-3">
            <input
              id="telemetryEnabled"
              type="checkbox"
              checked={privacySettings.telemetryEnabled}
              onChange={(e) => updatePrivacy({ telemetryEnabled: e.target.checked })}
              className="mt-1 w-4 h-4 text-[hsl(var(--accent))] bg-[hsl(var(--surface))] border-[hsl(var(--border-subtle))] rounded focus:ring-2 focus:ring-[hsl(var(--accent))]"
            />
            <div className="flex-1">
              <label
                htmlFor="telemetryEnabled"
                className="block text-sm font-medium text-[hsl(var(--text-primary))] cursor-pointer"
              >
                Send Anonymous Usage Statistics
              </label>
              <p className="text-xs text-[hsl(var(--text-secondary))] mt-2">
                Help us improve Lattice Lattice by sending anonymous usage data. This includes:
              </p>
              <ul className="mt-2 space-y-1 text-xs text-[hsl(var(--text-secondary))]">
                <li className="flex items-start gap-2">
                  <span className="text-[hsl(var(--accent))] mt-0.5">•</span>
                  <span>Feature usage frequency (which features you use most)</span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-[hsl(var(--accent))] mt-0.5">•</span>
                  <span>Performance metrics (app speed and responsiveness)</span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-[hsl(var(--accent))] mt-0.5">•</span>
                  <span>Operating system and version</span>
                </li>
              </ul>
              <p className="text-xs text-[hsl(var(--text-secondary))] mt-2 font-medium">
                We never collect: file names, content, search queries, or personally identifiable
                information.
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Crash Reporting */}
      <div className="space-y-4">
        <div className="flex items-center gap-2">
          <AlertTriangle className="w-4 h-4 text-[hsl(var(--accent))]" />
          <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">Crash Reporting</h3>
        </div>

        <div className="p-4 bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))]">
          <div className="flex items-start gap-3">
            <input
              id="crashReporting"
              type="checkbox"
              checked={privacySettings.crashReporting}
              onChange={(e) => updatePrivacy({ crashReporting: e.target.checked })}
              className="mt-1 w-4 h-4 text-[hsl(var(--accent))] bg-[hsl(var(--surface))] border-[hsl(var(--border-subtle))] rounded focus:ring-2 focus:ring-[hsl(var(--accent))]"
            />
            <div className="flex-1">
              <label
                htmlFor="crashReporting"
                className="block text-sm font-medium text-[hsl(var(--text-primary))] cursor-pointer"
              >
                Send Crash Reports
              </label>
              <p className="text-xs text-[hsl(var(--text-secondary))] mt-2">
                Automatically send crash reports to help us fix bugs and improve stability. Reports
                include:
              </p>
              <ul className="mt-2 space-y-1 text-xs text-[hsl(var(--text-secondary))]">
                <li className="flex items-start gap-2">
                  <span className="text-[hsl(var(--accent))] mt-0.5">•</span>
                  <span>Stack traces and error messages</span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-[hsl(var(--accent))] mt-0.5">•</span>
                  <span>System information (OS, memory, CPU)</span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-[hsl(var(--accent))] mt-0.5">•</span>
                  <span>Application state at time of crash</span>
                </li>
              </ul>
              <p className="text-xs text-[hsl(var(--text-secondary))] mt-2 font-medium">
                Crash reports are sanitized to remove any personal information or file content.
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Data Storage Info */}
      <div className="space-y-3">
        <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">Local Data Storage</h3>

        <div className="p-4 bg-[hsl(var(--surface))] rounded-lg space-y-3">
          <div>
            <div className="text-xs font-medium text-[hsl(var(--text-primary))] mb-1">Database</div>
            <div className="text-xs text-[hsl(var(--text-secondary))]">
              All your notes, files, and metadata are stored in a local SQLite database on your
              device.
            </div>
          </div>

          <div>
            <div className="text-xs font-medium text-[hsl(var(--text-primary))] mb-1">
              Vector Embeddings
            </div>
            <div className="text-xs text-[hsl(var(--text-secondary))]">
              Semantic search data is stored locally using pgvector. No cloud processing.
            </div>
          </div>

          <div>
            <div className="text-xs font-medium text-[hsl(var(--text-primary))] mb-1">
              AI Models
            </div>
            <div className="text-xs text-[hsl(var(--text-secondary))]">
              All AI models run locally on your device. Your data never leaves your computer.
            </div>
          </div>
        </div>
      </div>

      {/* Data Deletion */}
      <div className="p-4 bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))]">
        <h4 className="text-sm font-medium text-[hsl(var(--text-primary))] mb-2">Data Deletion</h4>
        <p className="text-xs text-[hsl(var(--text-secondary))] mb-3">
          You can delete all app data at any time by removing the application folder. Your
          original files remain untouched.
        </p>
        <div className="text-xs font-mono text-[hsl(var(--text-tertiary))] bg-[hsl(var(--surface-raised))] p-2 rounded">
          ~/.lattice-lattice/
        </div>
      </div>

      {/* Privacy Status Summary */}
      <div className="p-4 bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))]">
        <h4 className="text-sm font-medium text-[hsl(var(--text-primary))] mb-3">Current Status</h4>
        <div className="space-y-2">
          <div className="flex items-center justify-between text-xs">
            <span className="text-[hsl(var(--text-secondary))]">Usage Analytics</span>
            <span
              className={`font-medium ${
                privacySettings.telemetryEnabled
                  ? 'text-[hsl(var(--success-fg))]'
                  : 'text-[hsl(var(--text-tertiary))]'
              }`}
            >
              {privacySettings.telemetryEnabled ? 'Enabled' : 'Disabled'}
            </span>
          </div>
          <div className="flex items-center justify-between text-xs">
            <span className="text-[hsl(var(--text-secondary))]">Crash Reporting</span>
            <span
              className={`font-medium ${
                privacySettings.crashReporting
                  ? 'text-[hsl(var(--success-fg))]'
                  : 'text-[hsl(var(--text-tertiary))]'
              }`}
            >
              {privacySettings.crashReporting ? 'Enabled' : 'Disabled'}
            </span>
          </div>
          <div className="flex items-center justify-between text-xs">
            <span className="text-[hsl(var(--text-secondary))]">Data Processing</span>
            <span className="font-medium text-[hsl(var(--success-fg))]">100% Local</span>
          </div>
        </div>
      </div>

      {/* Links */}
      <div className="flex gap-4 text-xs">
        <button className="text-[hsl(var(--accent))] hover:underline">Privacy Policy</button>
        <button className="text-[hsl(var(--accent))] hover:underline">Data Usage FAQ</button>
        <button className="text-[hsl(var(--accent))] hover:underline">Contact Us</button>
      </div>
    </div>
  );
}
