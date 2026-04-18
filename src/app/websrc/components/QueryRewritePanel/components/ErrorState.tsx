import { Card } from '../../ui/card';

interface ErrorStateProps {
  error: string;
}

export function ErrorState({ error }: ErrorStateProps) {
  return (
    <Card className="border-[hsl(var(--danger-muted))] bg-[hsl(var(--danger-muted))]/20">
      <div className="p-4">
        <div className="flex items-start gap-3">
          <svg
            className="w-4 h-4 text-[hsl(var(--danger-fg))] flex-shrink-0 mt-0.5"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={1.75}
              d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          </svg>
          <div className="flex-1">
            <h4 className="text-sm font-semibold text-[hsl(var(--danger-fg))] mb-1">
              Couldn't generate suggestions
            </h4>
            <p className="text-sm text-[hsl(var(--danger-fg))]">{error}</p>
            <p className="text-xs text-[hsl(var(--danger-fg))] mt-2">
              Make sure Ollama is running and a model is loaded.
            </p>
          </div>
        </div>
      </div>
    </Card>
  );
}
