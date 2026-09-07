/**
 * SystemCapabilitiesCard
 *
 * This machine's RAM, CPU, GPU, and free disk, as four hairline rows. Used
 * in the model catalog so a user can judge what will fit.
 */

import { CATALOG_TEXT_BUTTON_CLASS } from './catalogUtils';
import { useModelCatalog } from '../../../hooks/useModelCatalog';
import { Skeleton } from '../../ui/Skeleton/Skeleton';

export function SystemCapabilitiesCard() {
  const {
    systemCapabilities: capabilities,
    capabilitiesLoading: loading,
    loadSystemCapabilities,
  } = useModelCatalog({
    autoLoadCapabilities: true,
    autoLoadModels: false,
  });

  if (loading) {
    return (
      <div className="border-t border-border-subtle">
        {[1, 2, 3, 4].map((i) => (
          <div key={i} className="flex items-center justify-between border-b border-border-subtle py-2.5">
            <Skeleton variant="text" width="20%" height="0.875rem" />
            <Skeleton variant="text" width="30%" height="0.875rem" />
          </div>
        ))}
      </div>
    );
  }

  if (!capabilities) {
    return (
      <div className="flex items-center gap-2 py-3">
        <p className="text-sm text-text-muted">Couldn&apos;t read this machine&apos;s specs.</p>
        <button
          type="button"
          onClick={() => void loadSystemCapabilities()}
          className={CATALOG_TEXT_BUTTON_CLASS}
        >
          Retry
        </button>
      </div>
    );
  }

  const gpu =
    capabilities.gpu_type !== 'None'
      ? `${capabilities.gpu_type} · ${capabilities.gpu_acceleration}${
          capabilities.vram_gb ? ` · ${capabilities.vram_gb.toFixed(1)} GB VRAM` : ''
        }`
      : 'Not detected';

  const rows: Array<[string, string]> = [
    ['Memory', `${capabilities.total_ram_gb.toFixed(1)} GB`],
    ['CPU', `${capabilities.cpu_cores} cores · ${capabilities.cpu_architecture}`],
    ['GPU', gpu],
    ['Free disk', `${capabilities.available_disk_gb.toFixed(1)} GB`],
  ];

  return (
    <div className="border-t border-border-subtle">
      {rows.map(([label, value]) => (
        <div key={label} className="flex items-center justify-between gap-6 border-b border-border-subtle py-2.5">
          <span className="text-sm text-text-secondary">{label}</span>
          <span className="text-sm tabular-nums text-text-primary">{value}</span>
        </div>
      ))}
    </div>
  );
}
