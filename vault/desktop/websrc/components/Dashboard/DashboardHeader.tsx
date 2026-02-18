import { Home } from 'lucide-react';

function getGreeting(): string {
  const hour = new Date().getHours();
  if (hour < 12) return 'Good morning';
  if (hour < 18) return 'Good afternoon';
  return 'Good evening';
}

function getCurrentDate(): string {
  return new Date().toLocaleDateString('en-US', {
    weekday: 'long',
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  });
}

export function DashboardHeader() {
  return (
    <header className="space-y-2">
      <h1 className="text-3xl font-bold gradient-text-brand flex items-center gap-3">
        <Home className="w-8 h-8 text-[var(--accent-primary)] drop-shadow-lg" />
        {getGreeting()}
      </h1>
      <p className="text-[var(--text-secondary)]">{getCurrentDate()}</p>
    </header>
  );
}
