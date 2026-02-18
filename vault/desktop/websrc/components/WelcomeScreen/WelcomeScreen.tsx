/**
 * WelcomeScreen - First-run experience orchestrator
 *
 * Purpose: Transform first-time anxiety into confident capability by guiding users
 * through essential setup steps while building trust and understanding.
 *
 * Flow:
 * 1. Welcome message (what is Recall/Vault?)
 * 2. Model download (required, shows progress)
 * 3. First folder indexing (suggested, can skip)
 * 4. Quick tour (optional, can skip)
 *
 * States: welcome, downloading, indexing, tour, complete
 * Accessibility: Keyboard navigation, screen reader support, progress announcements
 */

import { useState, useEffect } from 'react';

import { BookOpen, Download, FolderOpen, Sparkles, CheckCircle2, ArrowRight } from 'lucide-react';

import { FirstFolderPicker } from '../FirstFolderPicker';
import { ModelDownloadScreen } from '../ModelDownloadScreen';
import { QuickTour } from '../QuickTour';

export type OnboardingStep = 'welcome' | 'downloading' | 'indexing' | 'tour' | 'complete';

interface WelcomeScreenProps {
  onComplete: () => void;
}

interface StepIndicatorProps {
  current: number;
  total: number;
  labels: string[];
}

const StepIndicator = ({ current, total, labels }: StepIndicatorProps) => (
    <div className="flex items-center justify-center gap-2 mb-8">
      {Array.from({ length: total }).map((_, index) => (
        <div key={index} className="flex items-center">
          <div className="flex flex-col items-center">
            <div
              className={`w-10 h-10 rounded-full flex items-center justify-center border-2 transition-all duration-200 ${
                index < current
                  ? 'bg-[var(--accent-primary)] border-[var(--accent-primary)] text-white'
                  : index === current
                  ? 'border-[var(--accent-primary)] text-[var(--accent-primary)] scale-110'
                  : 'border-[var(--border-color)] text-[var(--text-tertiary)]'
              }`}
            >
              {index < current ? (
                <CheckCircle2 className="w-5 h-5" />
              ) : (
                <span className="text-sm font-semibold">{index + 1}</span>
              )}
            </div>
            <span className="text-xs text-[var(--text-secondary)] mt-1 whitespace-nowrap">
              {labels[index]}
            </span>
          </div>
          {index < total - 1 && (
            <div
              className={`w-16 h-0.5 mx-2 mb-6 transition-colors ${
                index < current ? 'bg-[var(--accent-primary)]' : 'bg-[var(--border-color)]'
              }`}
            />
          )}
        </div>
      ))}
    </div>
  );

export function WelcomeScreen({ onComplete }: WelcomeScreenProps) {
  const [step, setStep] = useState<OnboardingStep>('welcome');
  const [currentStepIndex, setCurrentStepIndex] = useState(0);
  const [isVisible, setIsVisible] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => setIsVisible(true), 10);
    return () => clearTimeout(timer);
  }, [step]);

  const stepLabels = ['Welcome', 'Download', 'Index', 'Tour'];

  useEffect(() => {
    const stepIndexMap: Record<OnboardingStep, number> = {
      welcome: 0,
      downloading: 1,
      indexing: 2,
      tour: 3,
      complete: 4,
    };
    setCurrentStepIndex(stepIndexMap[step]);
  }, [step]);

  const handleGetStarted = () => {
    setStep('downloading');
  };

  const handleDownloadComplete = () => {
    setStep('indexing');
  };

  const handleDownloadError = (error: string) => {
    console.error('Model download failed:', error);
  };

  const handleSkipIndexing = () => {
    setStep('tour');
  };

  const handleIndexingComplete = () => {
    setStep('tour');
  };

  const handleSkipTour = () => {
    completeOnboarding();
  };

  const handleTourComplete = () => {
    completeOnboarding();
  };

  const completeOnboarding = () => {
    localStorage.setItem('vault_onboarding_complete', 'true');
    setStep('complete');
    setTimeout(() => onComplete(), 500);
  };

  return (
    <div className="fixed inset-0 bg-[var(--bg-primary)] z-50 overflow-hidden">
      {step === 'welcome' && (
        <div
          key="welcome"
          className={`h-full flex items-center justify-center p-8 transition-opacity duration-300 ${isVisible ? 'opacity-100' : 'opacity-0'}`}
        >
            <div className="max-w-2xl w-full">
              <StepIndicator current={currentStepIndex} total={4} labels={stepLabels} />

              <div className="text-center mb-12 animate-in fade-in slide-in-from-bottom-5 duration-300" style={{ animationDelay: '100ms', animationFillMode: 'backwards' }}>
                <div className="inline-flex items-center justify-center w-20 h-20 gradient-brand rounded-2xl mb-6 shadow-lg">
                  <BookOpen className="w-10 h-10 text-white" />
                </div>
                <h1 className="text-4xl font-bold text-[var(--text-primary)] mb-4">
                  Welcome to Recall/Vault
                </h1>
                <p className="text-xl text-[var(--text-secondary)] mb-8">
                  Your intelligent document search companion
                </p>
              </div>

              <div className="grid gap-6 mb-12 animate-in fade-in slide-in-from-bottom-5 duration-300" style={{ animationDelay: '200ms', animationFillMode: 'backwards' }}>
                <FeatureCard
                  icon={<Sparkles className="w-6 h-6" />}
                  title="Semantic Search"
                  description="Find documents by meaning, not just keywords. Search naturally."
                />
                <FeatureCard
                  icon={<Download className="w-6 h-6" />}
                  title="Local & Private"
                  description="All processing happens on your machine. Your data never leaves."
                />
                <FeatureCard
                  icon={<FolderOpen className="w-6 h-6" />}
                  title="Automatic Indexing"
                  description="Point to your folders and we'll keep everything searchable."
                />
              </div>

              <div className="flex flex-col gap-3 animate-in fade-in slide-in-from-bottom-5 duration-300" style={{ animationDelay: '300ms', animationFillMode: 'backwards' }}>
                <button
                  onClick={handleGetStarted}
                  className="w-full px-6 py-4 bg-[var(--accent-primary)] text-white rounded-lg font-semibold text-lg hover:bg-[var(--accent-hover)] transition-colors shadow-md hover:shadow-lg flex items-center justify-center gap-2 group"
                  aria-label="Get started with Recall/Vault"
                >
                  Get Started
                  <ArrowRight className="w-5 h-5 transition-transform group-hover:translate-x-1" />
                </button>
                <p className="text-sm text-[var(--text-tertiary)] text-center">
                  Takes about 2 minutes to set up
                </p>
              </div>
            </div>
          </div>
        )}

        {step === 'downloading' && (
          <div
            key="downloading"
            className={`h-full transition-opacity duration-300 ${isVisible ? 'opacity-100' : 'opacity-0'}`}
          >
            <div className="max-w-2xl mx-auto pt-16 px-8">
              <StepIndicator current={currentStepIndex} total={4} labels={stepLabels} />
              <ModelDownloadScreen
                onComplete={handleDownloadComplete}
                onError={handleDownloadError}
                embedded
              />
            </div>
          </div>
        )}

        {step === 'indexing' && (
          <div
            key="indexing"
            className={`h-full transition-opacity duration-300 ${isVisible ? 'opacity-100' : 'opacity-0'}`}
          >
            <div className="max-w-2xl mx-auto pt-16 px-8">
              <StepIndicator current={currentStepIndex} total={4} labels={stepLabels} />
              <FirstFolderPicker onComplete={handleIndexingComplete} onSkip={handleSkipIndexing} />
            </div>
          </div>
        )}

        {step === 'tour' && (
          <div
            key="tour"
            className={`h-full transition-opacity duration-300 ${isVisible ? 'opacity-100' : 'opacity-0'}`}
          >
            <div className="max-w-2xl mx-auto pt-16 px-8">
              <StepIndicator current={currentStepIndex} total={4} labels={stepLabels} />
              <QuickTour onComplete={handleTourComplete} onSkip={handleSkipTour} />
            </div>
          </div>
        )}
    </div>
  );
}

interface FeatureCardProps {
  icon: React.ReactNode;
  title: string;
  description: string;
}

function FeatureCard({ icon, title, description }: FeatureCardProps) {
  return (
    <div className="flex items-start gap-4 p-6 bg-[var(--surface-elevated)] border border-[var(--border-color)] rounded-lg hover:border-[var(--accent-primary)] transition-all hover:shadow-md">
      <div className="flex-shrink-0 w-12 h-12 bg-[var(--accent-light)] rounded-lg flex items-center justify-center text-[var(--accent-primary)]">
        {icon}
      </div>
      <div>
        <h3 className="text-lg font-semibold text-[var(--text-primary)] mb-1">{title}</h3>
        <p className="text-sm text-[var(--text-secondary)]">{description}</p>
      </div>
    </div>
  );
}
