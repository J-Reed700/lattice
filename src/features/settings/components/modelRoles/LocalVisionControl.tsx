/**
 * Vision for a downloaded catalog chat model: its projector, fetched on
 * request into the model's folder, then the switch that lets the model see.
 * Drawn only when the catalog gives the model a projector.
 *
 * The projector runs through the ordinary download list, so its progress,
 * pause and failure are the ones every model download shows.
 */

import { useState } from 'react';

import { Eye } from 'lucide-react';

import { Switch } from '@/components/ui';
import { DownloadFailure } from '@/features/model/components/Downloads/DownloadFailure';
import { InlineDownloadStatus } from '@/features/model/components/Downloads/InlineDownloadStatus';
import { useDownloadPhase } from '@/features/model/hooks/useDownloadPhase';
import { useDownloadActions } from '@/features/model/hooks/useDownloads';
import { useDownloadState } from '@/features/model/hooks/useDownloadState';
import { latestDownloadFor } from '@/features/model/utils/downloadPhase';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import type { DownloadedVisionDto, JsonValue } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';
import type { DownloadedModel } from '@/types/downloadedModels';

import { useModelRoles } from './ModelRolesContext';
import { SECONDARY_BUTTON_CLASS, SWITCH_CLASS } from '../settingsStyles';

export function formatProjectorSize(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  return mb < 1024 ? `${mb.toFixed(0)} MB` : `${(mb / 1024).toFixed(1)} GB`;
}

export function LocalVisionControl({ model, vision }: { model: DownloadedModel; vision: DownloadedVisionDto }) {
  const { data: settings } = useSettingsQuery();
  const { mutateAsync: updateSettings, isPending: isSaving } = useUpdateSettingsMutation();
  const { downloads, downloadsSettled } = useDownloadState();
  const { startVisionProjectorDownload } = useDownloadActions();
  const { refresh, modelsFetchedAfterMount } = useModelRoles();
  const [isStarting, setIsStarting] = useState(false);

  const download = latestDownloadFor(downloads, vision.projector_path);
  // A finished projector changes what the model row reports; re-read it.
  const phase = useDownloadPhase({ ready: vision.projector_downloaded, download, poll: refresh });

  const startDownload = async () => {
    setIsStarting(true);
    try {
      const downloadId = await startVisionProjectorDownload(model.id);
      // None means it is already on disk: the model row only needs to catch up.
      if (downloadId === null) await refresh();
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

  if ((phase === 'live' || phase === 'finishing') && download) {
    return (
      <div className="mt-2">
        <InlineDownloadStatus download={download} finishing={phase === 'finishing'} label="Downloading the vision projector" />
      </div>
    );
  }

  return (
    <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
      <button
        type="button"
        onClick={() => void startDownload()}
        disabled={isStarting || !downloadsSettled || !modelsFetchedAfterMount}
        className={`${SECONDARY_BUTTON_CLASS} h-7 gap-1.5 px-2.5 text-xs`}
      >
        <Eye className="h-3.5 w-3.5" aria-hidden="true" />
        {isStarting ? 'Starting…' : `Download vision projector (${size})`}
      </button>
      {download?.state === 'Failed' ? (
        <DownloadFailure download={download} />
      ) : (
        <span className="text-text-muted">Lets this model see the pictures you attach.</span>
      )}
    </div>
  );
}
