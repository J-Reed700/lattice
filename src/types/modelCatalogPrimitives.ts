/**
 * Model Catalog Primitive Enums
 *
 * Leaf file with no internal imports — extracted from modelCatalog.ts
 * to break the cycle between api/models.ts and modelCatalog.ts.
 */

export type ModelCategory = import('../lib/bindings').ModelCategoryDto;

export type PerformanceTier = import('../lib/bindings').PerformanceTierDto;

export type GpuType = 'AppleSilicon' | 'Nvidia' | 'AMD' | 'Intel' | 'None';

export type GpuAcceleration = 'Metal' | 'CUDA' | 'ROCm' | 'Vulkan' | 'None';

export type CpuArchitecture = 'ARM64' | 'X86_64';

export type ModelSource = import('../lib/bindings').ModelSourceDto;

export type ModelSortBy = 'recommended' | 'popularity' | 'likes' | 'size_asc' | 'size_desc' | 'name';
