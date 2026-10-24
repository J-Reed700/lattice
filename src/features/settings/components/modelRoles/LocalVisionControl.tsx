import { useEffect, useState } from 'react';

import { Eye } from 'lucide-react';

import { Switch } from '@/components/ui';
import { useDownloadActions } from '@/features/model/hooks/useDownloads';
import { useDownloadState } from '@/features/model/hooks/useDownloadState';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import type { DownloadedVisionDto, JsonValue } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';
import type { DownloadedModel } from '@/types/downloadedModels';
import type { DownloadStatus } from '@/types/downloads';

import { useModelRoles } from './ModelRolesContext';
import { SECONDARY_BUTTON_CLASS, SWITCH_CLASS } from '../settingsStyles';

/** The projector's name beside the weights, as the backend writes it. */
const PROJECTOR_FILE = 'mmproj.gguf';

const ACTIVE: ReadonlySet<DownloadStatus['state']> = new Set(['Pending', 'Downloading', 'Paused']);

export function formatProjectorSize(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  return mb < 1024 ? `${mb.toFixed(0)} MB` : `${(mb / 1024).toFixed(1)} GB`;
}

const normalize = (path: string) => path.replace(/\\/g, '/');

/** Where the projector lands: the folder that holds the weights. */
function projectorPath(weights: string): string {
  const path = normalize(weights);
  return `${path.slice(0, path.lastIndexOf('/') + 1)}${PROJECTOR_FILE}`;
}

export function LocalVisionControl({ model, vision }: { model: DownloadedModel; vision: DownloadedVisionDto }) {
  const { data: settings } = useSettingsQuery();
  const { mutateAsync: updateSettings, isPending: isSaving } = useUpdateSettingsMutation();
  const { downloads } = useDownloadState();
  const { startVisionProjectorDownload } = useDownloadActions();
  const { refresh } = useModelRoles();
  const [startedId, setStartedId] = useState<string | null>(null);
  const [isStarting, setIsStarting] = useState(false);

  // The one this row started, else the latest download of this model's
  // projector (one started before the page was opened, say).
  const target = projectorPath(model.file_path);
  const download = downloads.find((candidate) => candidate.id === startedId)
    ?? downloads
      .filter((candidate) => normalize(candidate.destination) === target && candidate.state !== 'Cancelled')
      .sort((a, b) => b.created_at.localeCompare(a.created_at))[0]
    ?? null;
  const isDownloading = download !== null && ACTIVE.has(download.state);
  const completed = download?.state === 'Completed';

  // A finished projector changes what the model row reports; re-read it.
  useEffect(() => {
    if (completed && !vision.projector_downloaded) void refresh();
  }, [completed, vision.projector_downloaded, refresh]);

  const startDownload = async () => {
    setIsStarting(true);
    try {
      const downloadId = await startVisionProjectorDownload(model.id);
      // None means it is already on disk: the model row only needs to catch up.
      if (downloadId === null) await refresh();
      else setStartedId(downloadId);
    } catch (error) {
      toast.error("Couldn't download the vision projector", {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsStarting(false);
    }
  };

  const setEnabled = async (enabled: boolean) => {
    if (!settings?.llm) return;
    const configs = settings.llm.localModelConfigs ?? {};
    try {
      await updateSettings({
        category: 'llm',
        updates: {
          localModelConfigs: { ...configs, [model.model_id]: { ...configs[model.model_id], vision: enabled } },
        } as unknown as Record<string, JsonValue>,
      });
      await refresh();
    } catch (error) {
      toast.error("Couldn't change vision for this model", {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const size = formatProjectorSize(vision.projector_bytes);

  if (vision.projector_downloaded) {
    return (
      <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
        <label className="inline-flex items-center gap-2 text-text-secondary">
          <Switch
            aria-label="Let this model see images"
            checked={vision.enabled}
            disabled={isSaving}
            onCheckedChange={(checked) => void setEnabled(checked)}
            className={SWITCH_CLASS}
          />
          Let this model see images
        </label>
        <span className="tabular-nums text-text-muted">Vision projector · {size}</span>
      </div>
    );
  }

  if (isDownloading && download) {
    const percent = download.percentage ?? (download.total_bytes ? (download.bytes_downloaded / download.total_bytes) * 100 : null);
    return (
      <div className="mt-2 max-w-sm text-xs" role="status">
        <div className="flex items-center justify-between gap-3 text-text-secondary">
          <span>{download.state === 'Paused' ? 'Vision projector paused' : 'Downloading vision projector…'}</span>
          <span className="tabular-nums text-text-muted">
            {percent === null ? size : `${Math.round(percent)}% of ${size}`}
          </span>
        </div>
        <div
          className="mt-1 h-1 overflow-hidden rounded-full bg-surface-sunken"
          role="progressbar"
          aria-label="Vision projector download"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent === null ? undefined : Math.round(percent)}
        >
          <div
            className="h-full rounded-full bg-accent transition-[width] duration-base"
            style={{ width: `${percent ?? 0}%` }}
          />
        </div>
      </div>
    );
  }

  return (
    <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
      <button
        type="button"
        onClick={() => void startDownload()}
        disabled={isStarting}
        className={`${SECONDARY_BUTTON_CLASS} h-7 gap-1.5 px-2.5 text-xs`}
      >
        <Eye className="h-3.5 w-3.5" aria-hidden="true" />
        {isStarting ? 'Starting…' : `Download vision projector (${size})`}
      </button>
      {download?.state === 'Failed' ? (
        <span role="alert" className="text-danger-fg">
          The last attempt failed{download.error_message ? `: ${download.error_message}` : '.'}
        </span>
      ) : (
        <span className="text-text-muted">Lets this model see the pictures you attach.</span>
      )}
    </div>
  );
}
