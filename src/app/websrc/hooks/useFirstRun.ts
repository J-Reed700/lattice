/**
 * useFirstRun - Hook for detecting and managing first-run state
 *
 * Purpose: Determine if this is a user's first time launching the app by checking:
 * 1. Onboarding completion flag (localStorage)
 * 2. Model files existence (backend check)
 * 3. Database state (empty vs populated)
 *
 * Returns:
 * - isFirstRun: boolean indicating if onboarding should show
 * - isChecking: boolean indicating if we're still determining first-run status
 * - needsModels: boolean indicating if models need to be downloaded
 * - hasIndexedContent: boolean indicating if any content has been indexed
 */

import { useState, useEffect } from 'react';

import VaultAPI from '../lib/api';

interface FirstRunState {
  isFirstRun: boolean;
  isChecking: boolean;
  needsModels: boolean;
  hasIndexedContent: boolean;
}

export function useFirstRun(): FirstRunState {
  const [state, setState] = useState<FirstRunState>({
    isFirstRun: true,
    isChecking: true,
    needsModels: false,
    hasIndexedContent: false,
  });

  useEffect(() => {
    checkFirstRun();
  }, []);

  const checkFirstRun = async () => {
    try {
      // Check if user has explicitly completed onboarding
      const onboardingComplete = localStorage.getItem('vault_onboarding_complete') === 'true';

      if (onboardingComplete) {
        setState({
          isFirstRun: false,
          isChecking: false,
          needsModels: false,
          hasIndexedContent: true, // Assume if onboarding done, content indexed
        });
        return;
      }

      // For now, skip the onboarding entirely since models are loaded
      // TODO: Fix the API calls to be non-blocking
      setState({
        isFirstRun: false,
        isChecking: false,
        needsModels: false,
        hasIndexedContent: true,
      });

      // Run these in background (non-blocking)
      Promise.all([
        VaultAPI.initializeDatabase(),
        VaultAPI.getIndexingStats()
      ]).then(([dbResult, statsResult]) => {
        const needsModels = !dbResult.ok;
        const hasIndexedContent = statsResult.ok && statsResult.data.indexedDocuments > 0;

        // Only update if we actually need onboarding
        const shouldShowOnboarding = !onboardingComplete && (needsModels || !hasIndexedContent);

        if (shouldShowOnboarding) {
          setState({
            isFirstRun: shouldShowOnboarding,
            isChecking: false,
            needsModels,
            hasIndexedContent,
          });
        }
      }).catch(error => {
        console.error('Background check failed:', error);
        // Error already handled by setting isFirstRun to false
      });
    } catch (error) {
      console.error('Error checking first run status:', error);
      // On error, skip onboarding to at least show the UI
      setState({
        isFirstRun: false,
        isChecking: false,
        needsModels: false,
        hasIndexedContent: true,
      });
    }
  };

  return state;
}

/**
 * Utility function to reset first-run state (for testing)
 */
export function resetFirstRunState(): void {
  localStorage.removeItem('vault_onboarding_complete');
  localStorage.removeItem('vault_tour_dismissed');
}

/**
 * Utility function to mark onboarding as complete
 */
export function markOnboardingComplete(): void {
  localStorage.setItem('vault_onboarding_complete', 'true');
}

/**
 * Utility function to check if tour should be shown
 */
export function shouldShowTour(): boolean {
  const tourDismissed = localStorage.getItem('vault_tour_dismissed') === 'true';
  const onboardingComplete = localStorage.getItem('vault_onboarding_complete') === 'true';
  return !tourDismissed && !onboardingComplete;
}
