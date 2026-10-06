import type { ModelMetadata } from '../../../types/modelCatalog';

/** Read the artifact, not a repository's list of advertised formats. */
export function modelQuantization(model: Pick<ModelMetadata, 'default_filename' | 'supported_quantizations'>): string | null {
  const match = model.default_filename?.match(/(?:^|[.\-_ ])((?:UD-)?(?:IQ[1-8]|Q[1-8]|TQ[12])(?:_[A-Z0-9]+)*|BF16|F16|F32|FP16|FP32|MXFP4)(?:[.\- ]|$)/i);
  if (match) return match[1].toUpperCase();
  return model.supported_quantizations.length === 1 ? model.supported_quantizations[0] : null;
}

export function quantizationDescription(quantization: string | null): string {
  if (!quantization) return 'Precision not reported';
  const value = quantization.replace(/^UD-/, '');
  if (/^(B?F|FP)(16|32)$/.test(value)) return 'Floating-point weights · largest downloads';
  const bits = value.match(/^(?:I?Q|TQ)(\d)/)?.[1];
  if (bits && Number(bits) <= 3) return 'More compression · greater quality tradeoff';
  if (bits && Number(bits) <= 5) return 'Moderate compression · smaller download';
  if (bits) return 'Less compression · larger download';
  return 'See the publisher’s model card for details';
}

export function hasModelVersions(model: ModelMetadata): boolean {
  return model.category === 'LLM' && Boolean(model.model_id) && model.format !== 'safetensors';
}
