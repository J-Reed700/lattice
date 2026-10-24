import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SystemCapabilitiesCard } from './SystemCapabilitiesCard';

const h = vi.hoisted(() => ({ catalog: {} as Record<string, any> }));
vi.mock('@/features/model/hooks/useModelCatalog', () => ({ useModelCatalog: () => h.catalog }));

const machine = {
  total_ram_gb: 31.6, cpu_cores: 10, cpu_architecture: 'arm64', gpu_type: 'AppleSilicon', gpu_acceleration: 'Metal',
  vram_gb: null, available_disk_gb: 212.34,
};

describe('SystemCapabilitiesCard', () => {
  beforeEach(() => {
    h.catalog = { systemCapabilities: machine, capabilitiesLoading: false, loadSystemCapabilities: vi.fn().mockResolvedValue(undefined) };
  });

  it('lists memory, CPU, GPU and free disk', () => {
    render(<SystemCapabilitiesCard />);
    expect(screen.getByText('31.6 GB')).toBeInTheDocument();
    expect(screen.getByText('10 cores · arm64')).toBeInTheDocument();
    expect(screen.getByText('AppleSilicon · Metal')).toBeInTheDocument();
    expect(screen.getByText('212.3 GB')).toBeInTheDocument();
  });

  it('adds dedicated VRAM when there is some', () => {
    h.catalog.systemCapabilities = { ...machine, gpu_type: 'Nvidia', gpu_acceleration: 'Cuda', vram_gb: 12 };
    render(<SystemCapabilitiesCard />);
    expect(screen.getByText('Nvidia · Cuda · 12.0 GB VRAM')).toBeInTheDocument();
  });

  it('says when no GPU was found', () => {
    h.catalog.systemCapabilities = { ...machine, gpu_type: 'None' };
    render(<SystemCapabilitiesCard />);
    expect(screen.getByText('Not detected')).toBeInTheDocument();
  });

  it('shows placeholders while reading the machine', () => {
    h.catalog.capabilitiesLoading = true;
    render(<SystemCapabilitiesCard />);
    expect(screen.queryByText('Memory')).not.toBeInTheDocument();
  });

  it('offers a retry when the specs could not be read', () => {
    h.catalog.systemCapabilities = null;
    render(<SystemCapabilitiesCard />);
    expect(screen.getByText("Couldn't read this machine's specs.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(h.catalog.loadSystemCapabilities).toHaveBeenCalled();
  });
});
