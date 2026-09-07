import { describe, it, expect } from 'vitest';

import { computeModelFit, FIT_LABEL } from './catalogUtils';

import type { SystemCapabilities } from '../../../types/api/models';

function caps(overrides: Partial<SystemCapabilities> = {}): SystemCapabilities {
  return {
    total_ram_gb: 32,
    cpu_cores: 10,
    cpu_architecture: 'ARM64',
    gpu_type: 'AppleSilicon',
    gpu_acceleration: 'Metal',
    vram_gb: null,
    available_disk_gb: 500,
    ...overrides,
  };
}

function model(minimumRamGb: number, sizeGb = 0) {
  return { minimum_ram_gb: minimumRamGb, size_gb: sizeGb };
}

describe('computeModelFit', () => {
  it('says nothing without capabilities', () => {
    expect(computeModelFit(model(8, 4.4), null)).toBeNull();
    expect(computeModelFit(model(8, 4.4), undefined)).toBeNull();
  });

  it('says nothing when the machine reports no RAM', () => {
    expect(computeModelFit(model(8, 4.4), caps({ total_ram_gb: 0 }))).toBeNull();
  });

  it('says nothing when the model states no requirement', () => {
    expect(computeModelFit(model(0, 4.4), caps())).toBeNull();
    expect(computeModelFit(model(Number.NaN, 4.4), caps())).toBeNull();
  });

  it('fits a small model on a 32 GB Apple Silicon machine', () => {
    const fit = computeModelFit(model(8, 4.4), caps());
    expect(fit?.verdict).toBe('fits');
    expect(fit?.usableMemoryGb).toBeCloseTo(22.4, 5);
    expect(fit?.reason).toBe('Needs 8 GB · 22.4 GB usable');
  });

  it('is tight when the requirement lands between 80% and 100% of usable', () => {
    // 16 GB Apple Silicon → usable 11.2; the fits/tight boundary is 8.96.
    const fit = computeModelFit(model(10, 6), caps({ total_ram_gb: 16 }));
    expect(fit?.verdict).toBe('tight');
  });

  it('is too large when the requirement exceeds usable', () => {
    const fit = computeModelFit(model(32, 20), caps({ total_ram_gb: 16 }));
    expect(fit?.verdict).toBe('too-large');
  });

  it('ignores VRAM on Apple Silicon — unified memory has one pool', () => {
    // Were the 8 GB VRAM figure believed, usable would be 8 and this would read
    // "tight". Unified memory means the pool is 70% of the 16 GB of system RAM.
    const fit = computeModelFit(model(7), caps({ total_ram_gb: 16, vram_gb: 8 }));
    expect(fit?.verdict).toBe('fits');
    expect(fit?.usableMemoryGb).toBeCloseTo(11.2, 5);
  });

  it('uses VRAM on an accelerated discrete GPU', () => {
    const fit = computeModelFit(
      model(16, 12),
      caps({
        total_ram_gb: 64,
        gpu_type: 'Nvidia',
        gpu_acceleration: 'CUDA',
        vram_gb: 24,
      }),
    );
    expect(fit?.verdict).toBe('fits');
    expect(fit?.usableMemoryGb).toBe(24);
  });

  it('is tight when the requirement nearly fills VRAM', () => {
    const fit = computeModelFit(
      model(22),
      caps({
        total_ram_gb: 64,
        gpu_type: 'Nvidia',
        gpu_acceleration: 'CUDA',
        vram_gb: 24,
      }),
    );
    expect(fit?.verdict).toBe('tight');
  });

  it('falls back to RAM when there is no acceleration, even with VRAM reported', () => {
    const fit = computeModelFit(
      model(16),
      caps({
        total_ram_gb: 64,
        gpu_type: 'Nvidia',
        gpu_acceleration: 'None',
        vram_gb: 24,
      }),
    );
    expect(fit?.verdict).toBe('fits');
    expect(fit?.usableMemoryGb).toBeCloseTo(44.8, 5);
  });

  it('budgets 70% of RAM on an Intel machine with no VRAM', () => {
    const fit = computeModelFit(
      model(8),
      caps({
        total_ram_gb: 16,
        gpu_type: 'Intel',
        gpu_acceleration: 'None',
        vram_gb: null,
      }),
    );
    expect(fit?.verdict).toBe('fits');
    expect(fit?.usableMemoryGb).toBeCloseTo(11.2, 5);
  });

  it('condemns a model that will not fit on disk, and names the disk', () => {
    const fit = computeModelFit(
      model(8, 24),
      caps({ total_ram_gb: 64, available_disk_gb: 2 }),
    );
    expect(fit?.verdict).toBe('too-large');
    expect(fit?.reason).toBe('Needs 24 GB on disk · 2 GB free');
  });

  it('skips the disk gate entirely when the disk probe reported nothing', () => {
    const fit = computeModelFit(
      model(8, 24),
      caps({ total_ram_gb: 64, available_disk_gb: 0 }),
    );
    expect(fit?.verdict).toBe('fits');
  });

  it('treats the 80% boundary as fitting', () => {
    // usable = 0.7 * 40 = 28; 0.8 * 28 = 22.4
    const fit = computeModelFit(model(22.4), caps({ total_ram_gb: 40 }));
    expect(fit?.verdict).toBe('fits');
  });

  it('treats the exact-usable boundary as tight', () => {
    const fit = computeModelFit(model(28), caps({ total_ram_gb: 40 }));
    expect(fit?.verdict).toBe('tight');
  });
});

describe('FIT_LABEL', () => {
  it('is one word per verdict', () => {
    expect(FIT_LABEL.fits).toBe('Fits');
    expect(FIT_LABEL.tight).toBe('Tight');
    expect(FIT_LABEL['too-large']).toBe('Too large');
  });
});
