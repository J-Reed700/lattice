// Leaf file extracted from modelCatalog.ts to break the circular import with api/models.ts.

export type ModelCategory = 'LLM' | 'Embedding' | 'OCR' | 'Transcription';

export type PerformanceTier = 'Fast' | 'Balanced' | 'Accurate';

export type GpuType = 'AppleSilicon' | 'Nvidia' | 'AMD' | 'Intel' | 'None';

export type GpuAcceleration = 'Metal' | 'CUDA' | 'ROCm' | 'Vulkan' | 'None';

export type CpuArchitecture = 'ARM64' | 'X86_64';

export type CompatibilityLevel = 'Incompatible' | 'Poor' | 'Good' | 'Excellent';

export type ModelSource = 'Curated' | 'External';

export type ModelSortBy = 'recommended' | 'popularity' | 'likes' | 'size_asc' | 'size_desc' | 'name';
