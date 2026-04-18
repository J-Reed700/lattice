/**
 * QuickTour - Interactive tutorial with spotlight tooltips
 *
 * Purpose: Transform the question "what can I do?" into confident mastery through
 * contextual, non-intrusive education. Users need to discover capabilities without
 * feeling overwhelmed or lectured.
 *
 * Features:
 * - 3-4 key tooltips (search, results, settings)
 * - Spotlight effect highlighting each feature
 * - Sample search demonstration
 * - Dismissible with "don't show again"
 * - Beautiful overlay with smooth transitions
 * - Optional - can skip entirely
 *
 * States: intro, tooltip-1, tooltip-2, tooltip-3, demo, complete
 * Accessibility: Keyboard navigation, clear focus, screen reader announcements
 */

import { useState, useEffect } from 'react';

import {
  Search,
  FileText,
  Settings,
  Zap,
  ArrowRight,
  X,
  CheckCircle2,
  Sparkles,
} from 'lucide-react';

interface QuickTourProps {
  onComplete: () => void;
  onSkip: () => void;
}

type TourStep = 'intro' | 'search' | 'results' | 'settings' | 'shortcuts' | 'demo' | 'complete';

interface Tooltip {
  id: TourStep;
  title: string;
  description: string;
  icon: React.ReactNode;
  position: { top?: string; bottom?: string; left?: string; right?: string };
  spotlightPosition: { top?: string; bottom?: string; left?: string; right?: string; width: string; height: string };
  action?: string;
}

export function QuickTour({ onComplete, onSkip }: QuickTourProps) {
  const [currentStep, setCurrentStep] = useState<TourStep>('intro');
  const [dontShowAgain, setDontShowAgain] = useState(false);
  const [isVisible, setIsVisible] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => setIsVisible(true), 10);
    return () => clearTimeout(timer);
  }, [currentStep]);

  const tooltips: Record<TourStep, Tooltip> = {
    intro: {
      id: 'intro',
      title: 'Quick Tour',
      description: 'Let me show you around in 30 seconds',
      icon: <Sparkles className="w-6 h-6" />,
      position: { top: '50%', left: '50%' },
      spotlightPosition: { top: '50%', left: '50%', width: '400px', height: '300px' },
    },
    search: {
      id: 'search',
      title: 'Search Anywhere',
      description:
        'Press Cmd+K (Mac) or Ctrl+K (Windows) to open search from anywhere. Type naturally - semantic search understands meaning, not just keywords.',
      icon: <Search className="w-6 h-6" />,
      position: { top: '20%', left: '50%' },
      spotlightPosition: { top: '10%', left: '25%', width: '50%', height: '80px' },
      action: 'Try pressing Cmd+K now',
    },
    results: {
      id: 'results',
      title: 'Smart Results',
      description:
        'Results show relevance scores and snippets. Click any result to open the full document. Recent searches appear here for quick access.',
      icon: <FileText className="w-6 h-6" />,
      position: { top: '40%', left: '50%' },
      spotlightPosition: { top: '30%', left: '15%', width: '70%', height: '300px' },
    },
    settings: {
      id: 'settings',
      title: 'Settings & Folders',
      description:
        'Manage indexed folders, adjust search preferences, and configure auto-indexing. Add more folders anytime to expand your searchable content.',
      icon: <Settings className="w-6 h-6" />,
      position: { top: '60%', left: '50%' },
      spotlightPosition: { top: '5%', right: '5%', width: '60px', height: '60px' },
    },
    shortcuts: {
      id: 'shortcuts',
      title: 'Keyboard Shortcuts',
      description:
        'Cmd+K: Search • Cmd+,: Settings • Esc: Close • ↑↓: Navigate results. Master these for lightning-fast workflow.',
      icon: <Zap className="w-6 h-6" />,
      position: { top: '50%', left: '50%' },
      spotlightPosition: { top: '50%', left: '50%', width: '400px', height: '250px' },
    },
    demo: {
      id: 'demo',
      title: 'Try It Out!',
      description:
        'Search for something in your documents. Try a question like "meeting notes from last week" or "project proposal draft".',
      icon: <Sparkles className="w-6 h-6" />,
      position: { top: '30%', left: '50%' },
      spotlightPosition: { top: '10%', left: '20%', width: '60%', height: '500px' },
      action: 'Open search and try it',
    },
    complete: {
      id: 'complete',
      title: 'You\'re All Set!',
      description: 'You now know the essentials. Start searching and discover what Vault can do.',
      icon: <CheckCircle2 className="w-6 h-6" />,
      position: { top: '50%', left: '50%' },
      spotlightPosition: { top: '50%', left: '50%', width: '400px', height: '300px' },
    },
  };

  const tourSequence: TourStep[] = ['intro', 'search', 'results', 'settings', 'shortcuts', 'demo', 'complete'];
  const currentIndex = tourSequence.indexOf(currentStep);
  const isLastStep = currentStep === 'complete';

  const handleNext = () => {
    if (isLastStep) {
      handleComplete();
    } else {
      const nextIndex = currentIndex + 1;
      setCurrentStep(tourSequence[nextIndex]);
    }
  };

  const handleSkip = () => {
    if (dontShowAgain) {
      localStorage.setItem('vault_tour_dismissed', 'true');
    }
    onSkip();
  };

  const handleComplete = () => {
    if (dontShowAgain) {
      localStorage.setItem('vault_tour_dismissed', 'true');
    }
    setCurrentStep('complete');
    setTimeout(() => {
      onComplete();
    }, 1500);
  };

  const tooltip = tooltips[currentStep];

  return (
    <div className="fixed inset-0 z-50">
      {/* Backdrop with spotlight effect */}
      <div
        className={`absolute inset-0 bg-[hsl(var(--overlay))] transition-opacity duration-base ${isVisible ? 'opacity-100' : 'opacity-0'}`}
        style={{
          background: 'radial-gradient(circle at var(--spotlight-x, 50%) var(--spotlight-y, 50%), transparent 200px, rgba(0,0,0,0.8) 400px)',
        }}
      />

      {/* Spotlight ring */}
      {currentStep !== 'intro' && currentStep !== 'complete' && (
        <div
          key={currentStep}
          className={`absolute border-2 border-[hsl(var(--accent))] rounded-md pointer-events-none shadow-md transition-opacity duration-base ${isVisible ? 'opacity-100 scale-100' : 'opacity-0 scale-90'}`}
          style={{
            top: tooltip.spotlightPosition.top,
            left: tooltip.spotlightPosition.left,
            right: tooltip.spotlightPosition.right,
            width: tooltip.spotlightPosition.width,
            height: tooltip.spotlightPosition.height,
            transform: 'translate(-50%, -50%)',
            boxShadow: '0 0 0 9999px rgba(0, 0, 0, 0.75), 0 0 50px rgba(59, 130, 246, 0.5)',
          }}
        />
      )}

      {/* Tooltip card */}
      <div
        key={currentStep}
        className={`absolute max-w-md w-full mx-4 bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] rounded-lg shadow-md overflow-hidden transition-opacity duration-base ${isVisible ? 'opacity-100 translate-y-0 scale-100' : 'opacity-0 translate-y-5 scale-95'}`}
        style={{
          top: currentStep === 'intro' || currentStep === 'complete' ? '50%' : tooltip.position.top,
          left: currentStep === 'intro' || currentStep === 'complete' ? '50%' : tooltip.position.left,
          bottom: tooltip.position.bottom,
          right: tooltip.position.right,
          transform:
            currentStep === 'intro' || currentStep === 'complete'
              ? 'translate(-50%, -50%)'
              : 'translateX(-50%)',
        }}
      >
          {/* Header */}
          <div className="flex items-center justify-between p-6 border-b border-[hsl(var(--border-subtle))]">
            <div className="flex items-center gap-3">
              <div className="w-12 h-12 bg-[hsl(var(--accent))] rounded-lg flex items-center justify-center text-[hsl(var(--accent-fg))]">
                {tooltip.icon}
              </div>
              <div>
                <h3 className="text-xl font-bold text-[hsl(var(--text-primary))]">{tooltip.title}</h3>
                <p className="text-xs text-[hsl(var(--text-tertiary))]">
                  Step {currentIndex + 1} of {tourSequence.length}
                </p>
              </div>
            </div>
            <button
              onClick={handleSkip}
              className="text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))] transition-colors"
              aria-label="Skip tour"
            >
              <X className="w-5 h-5" />
            </button>
          </div>

          {/* Content */}
          <div className="p-6">
            <p className="text-[hsl(var(--text-secondary))] mb-6 leading-relaxed">
              {tooltip.description}
            </p>

            {tooltip.action && (
              <div className="mb-6 p-3 bg-[hsl(var(--accent-muted))] border border-[hsl(var(--accent))] rounded-lg">
                <p className="text-sm font-medium text-[hsl(var(--accent))] flex items-center gap-2">
                  <Zap className="w-4 h-4" />
                  {tooltip.action}
                </p>
              </div>
            )}

            {currentStep === 'complete' && (
              <div className="flex justify-center mb-6 animate-in zoom-in duration-300" style={{ animationDelay: '200ms' }}>
                <CheckCircle2 className="w-16 h-16 text-[hsl(var(--success-fg))]" />
              </div>
            )}

            {/* Progress bar */}
            <div className="mb-6">
              <div className="flex justify-between text-xs text-[hsl(var(--text-tertiary))] mb-2">
                <span>Progress</span>
                <span>
                  {currentIndex + 1}/{tourSequence.length}
                </span>
              </div>
              <div className="w-full bg-[hsl(var(--surface))] rounded-full h-2">
                <div
                  className="bg-[hsl(var(--accent))] h-2 rounded-full transition-colors duration-base ease-out"
                  style={{ width: `${((currentIndex + 1) / tourSequence.length) * 100}%` }}
                />
              </div>
            </div>

            {/* Actions */}
            <div className="flex gap-3">
              {currentStep !== 'complete' && (
                <>
                  <label className="flex items-center gap-2 text-sm text-[hsl(var(--text-secondary))] cursor-pointer flex-1">
                    <input
                      type="checkbox"
                      checked={dontShowAgain}
                      onChange={(e) => setDontShowAgain(e.target.checked)}
                      className="rounded border-[hsl(var(--border-subtle))] text-[hsl(var(--accent))] focus:ring-[hsl(var(--accent))]"
                    />
                    Don't show again
                  </label>
                  <button
                    onClick={handleNext}
                    className="px-6 py-3 bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] rounded-md hover:bg-[hsl(var(--accent-hover))] transition-colors duration-fast font-medium shadow-sm flex items-center gap-2"
                  >
                    {isLastStep ? 'Finish' : 'Next'}
                    <ArrowRight className="w-4 h-4" strokeWidth={1.75} />
                  </button>
                </>
              )}
            </div>
          </div>
        </div>

      {/* Skip button (bottom center) */}
      {currentStep !== 'complete' && (
        <button
          onClick={handleSkip}
          className="absolute bottom-8 left-1/2 -translate-x-1/2 px-6 py-3 bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] text-[hsl(var(--text-primary))] rounded-md hover:border-[hsl(var(--accent))] transition-colors duration-fast shadow-sm animate-in fade-in slide-in-from-bottom-5 duration-base"
          style={{ animationDelay: '500ms', animationFillMode: 'backwards' }}
        >
          Skip tour
        </button>
      )}
    </div>
  );
}
