import { useAppNavigate } from '@/hooks/useAppNavigate';
import { MODEL_SETTINGS_PATH } from '@/lib/settingsLinks';

export function OpenModelSettingsButton({ className = 'min-h-9 rounded-full border border-accent/40 bg-background px-3 text-xs font-medium text-text-primary' }: { className?: string }) {
  const navigate = useAppNavigate();
  return <button type="button" onClick={() => navigate(MODEL_SETTINGS_PATH)} className={className}>Open model settings</button>;
}
