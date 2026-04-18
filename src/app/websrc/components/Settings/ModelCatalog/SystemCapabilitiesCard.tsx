/**
 * SystemCapabilitiesCard
 *
 * Displays system hardware capabilities (RAM, GPU, CPU, disk space)
 * Used for showing user's system specs in the model catalog
 */

import { useEffect } from 'react';

import { Cpu, HardDrive, Zap, Server, AlertCircle } from 'lucide-react';

import { useModelCatalogStore, selectSystemCapabilities, selectCapabilitiesLoading } from '../../../stores/modelCatalogStore';
import Card, { CardHeader, CardTitle, CardContent } from '../../ui/Card/Card';
import { Skeleton } from '../../ui/Skeleton/Skeleton';

export function SystemCapabilitiesCard() {
  const capabilities = useModelCatalogStore(selectSystemCapabilities);
  const loading = useModelCatalogStore(selectCapabilitiesLoading);
  const loadCapabilities = useModelCatalogStore((state) => state.loadSystemCapabilities);

  useEffect(() => {
    if (!capabilities && !loading) {
      loadCapabilities();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [capabilities, loading]);

  if (loading) {
    return (
      <Card padding="md">
        <CardHeader>
          <div className="flex items-center gap-2">
            <Server className="w-5 h-5 text-[hsl(var(--accent))]" />
            <CardTitle>System Capabilities</CardTitle>
          </div>
        </CardHeader>
        <CardContent>
          <div className="space-y-4 mt-4">
            {[1, 2, 3, 4].map((i) => (
              <div key={i} className="flex items-center justify-between">
                <Skeleton variant="text" width="40%" height="1rem" />
                <Skeleton variant="text" width="30%" height="1rem" />
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    );
  }

  if (!capabilities) {
    return (
      <Card padding="md">
        <CardContent>
          <div className="flex items-center gap-3 text-[hsl(var(--text-secondary))]">
            <AlertCircle className="w-5 h-5 text-[hsl(var(--warning-fg))]" />
            <span className="text-sm">Failed to detect system capabilities</span>
          </div>
        </CardContent>
      </Card>
    );
  }

  const items = [
    {
      icon: Server,
      label: 'RAM',
      value: `${capabilities.total_ram_gb.toFixed(1)} GB`,
      color: 'text-blue-500',
    },
    {
      icon: Cpu,
      label: 'CPU',
      value: `${capabilities.cpu_cores} cores (${capabilities.cpu_architecture})`,
      color: 'text-purple-500',
    },
    {
      icon: Zap,
      label: 'GPU',
      value: capabilities.gpu_type !== 'None'
        ? `${capabilities.gpu_type} (${capabilities.gpu_acceleration}${capabilities.vram_gb ? `, ${capabilities.vram_gb.toFixed(1)} GB VRAM` : ''})`
        : 'Not detected',
      color: capabilities.gpu_type !== 'None' ? 'text-green-500' : 'text-[hsl(var(--text-tertiary))]',
    },
    {
      icon: HardDrive,
      label: 'Disk Space',
      value: `${capabilities.available_disk_gb.toFixed(1)} GB available`,
      color: 'text-orange-500',
    },
  ];

  return (
    <Card padding="md">
      <CardHeader>
        <div className="flex items-center gap-2">
          <Server className="w-5 h-5 text-[hsl(var(--accent))]" />
          <CardTitle className="text-base">System Capabilities</CardTitle>
        </div>
      </CardHeader>
      <CardContent>
        <div className="space-y-4 mt-4">
          {items.map((item) => {
            const Icon = item.icon;
            return (
              <div key={item.label} className="flex items-start gap-3">
                <div className={`mt-0.5 ${item.color}`}>
                  <Icon className="w-4 h-4" />
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                    {item.label}
                  </div>
                  <div className="text-sm text-[hsl(var(--text-primary))] mt-0.5 break-words">
                    {item.value}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}
