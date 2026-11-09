import type * as Wire from '@/lib/bindings';

export interface ExamplesTableProps {
  signature: Wire.ProblemSignature;
  examples: Wire.CodingExampleDto[];
  coverage: Wire.ExampleCoverageDto;
  busy?: boolean;
  onCheck: (example: { ordinal: number | null; args: string[]; expected: string }) => Promise<void>;
  onRemove: (ordinal: number) => Promise<void>;
}

export function ExamplesTable(_props: ExamplesTableProps): JSX.Element | null {
  return null;
}
