import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SearchTab } from './SearchTab';
import { VaultAPI } from '../../lib/api';

import type { AppSettings } from '../../types/api/settings';

const baseSettings: AppSettings = {
  indexing: {
    chunkSize: 1000,
    chunkOverlap: 200,
    batchSize: 32,
    autoIndexNewFiles: true,
    fileTypes: ['pdf'],
    excludedPaths: [],
  },
  search: {
    maxResults: 10,
    similarityThreshold: 0.7,
    enableReranking: true,
    hybridSearchAlpha: 0.5,
    retrievalTuning: {
      kbSearchMinLimit: 8,
      kbSearchMaxLimit: 48,
      docShortlistCandidateMin: 16,
      docShortlistCandidateMax: 192,
      docShortlistDocMin: 4,
      docShortlistDocMax: 24,
      shortlistGateMinCandidates: 6,
      shortlistGateMinDocs: 2,
      wikiSearchMaxResults: 5,
      wikiSnippetMaxChars: 360,
      wikiContextLimit: 5,
      webSearchMaxResults: 5,
      webSnippetMaxChars: 500,
      deepResearchDepth: 3,
      deepResearchBranchQueries: 3,
      externalSearchMaxWikiTerms: 8,
      externalSearchMaxWebTerms: 12,
      externalSearchQueryMaxChars: 220,
      rerankMaxCandidates: 12,
      rerankQueryMaxChars: 1000,
      overlapMinHitsForMultiTerm: 2,
      docSupportMultiHitRatioFactor: 0.35,
      docSupportSingleHitRatioFactor: 0.65,
      docSupportMultiHitRatioMin: 0.18,
      docSupportMultiHitRatioMax: 0.4,
      docSupportSingleHitRatioMin: 0.3,
      docSupportSingleHitRatioMax: 0.55,
    },
  },
  llm: {
    provider: 'auto',
    model: 'gpt-oss:20b',
    temperature: 0.7,
    topP: 0.9,
    topK: 40,
    repeatPenalty: 1.1,
    maxTokens: 2048,
    contextWindow: 8192,
    ollamaUrl: 'http://localhost:11434',
    ollamaAuthHeaderName: '',
    ollamaAuthHeaderValue: '',
    timeoutSeconds: 30,
    streamResponses: true,
    prompts: {
      systemPrompt: '',
      greetingPromptTemplate: '',
      ragPromptTemplate: '',
      noContextPromptTemplate: '',
      toolFollowupPromptTemplate: '',
    },
    verification: { enabled: false },
    toolOutput: {
      maxChars: 2000,
      excerptChars: 220,
      maxResults: 5,
      highlightTermsMax: 6,
      templates: {
        defaultTemplate: '',
        getDocumentTemplate: '',
        semanticSearchTemplate: '',
      },
    },
    router: {
      enabled: false,
      model: 'phi',
      timeoutMs: 7000,
      maxTokens: 256,
      temperature: 0.1,
      ambiguityThreshold: 0.5,
      preferLastDocument: false,
      promptTemplate: '',
      clarifyPromptTemplate: '',
    },
    externalModelDirectories: [],
    customTools: [],
  },
  ui: {
    theme: 'system',
    fontSize: 14,
    showPreview: true,
    resultsPerPage: 20,
    enableAnimations: true,
  },
  sync: {
    syncEnabled: false,
    syncUrl: '',
    syncIntervalMinutes: 5,
    autoSync: false,
    syncOnStartup: false,
  },
  backup: {
    autoBackupEnabled: false,
    backupFrequency: 'daily',
    backupRetentionDays: 7,
    backupPath: '',
    compressBackups: true,
  },
};

describe('SearchTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(VaultAPI, 'getSettings').mockResolvedValue({
      ok: true,
      data: structuredClone(baseSettings),
    });
    vi.spyOn(VaultAPI, 'updateSettings').mockResolvedValue({
      ok: true,
      data: structuredClone(baseSettings),
    });
  });

  it('loads and renders backend search settings', async () => {
    render(<SearchTab />);

    expect(await screen.findByText('Search Settings')).toBeInTheDocument();
    expect(screen.getByText('Core Retrieval')).toBeInTheDocument();
    expect(screen.getByDisplayValue('10')).toBeInTheDocument();
  });

  it('persists reranking toggle', async () => {
    const user = userEvent.setup();
    render(<SearchTab />);

    const checkbox = (await screen.findByLabelText(/Enable reranking/i)) as HTMLInputElement;
    expect(checkbox).toBeChecked();

    await user.click(checkbox);

    await waitFor(() => {
      expect(VaultAPI.updateSettings).toHaveBeenCalledWith({
        category: 'search',
        updates: { enableReranking: false },
      });
    });
  });

  it('shows advanced tuning and persists tuning fields', async () => {
    render(<SearchTab />);

    const advancedToggle = await screen.findByRole('button', {
      name: /Advanced Retrieval Tuning/i,
    });
    fireEvent.click(advancedToggle);

    const field = await screen.findByLabelText(/KB Search Min Limit/i);
    fireEvent.change(field, { target: { value: '12' } });
    fireEvent.blur(field);

    await waitFor(() => {
      expect(VaultAPI.updateSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          category: 'search',
          updates: expect.objectContaining({
            retrievalTuning: expect.objectContaining({
              kbSearchMinLimit: 12,
            }),
          }),
        })
      );
    });
  });

  it('normalizes paired bounds before persisting tuning', async () => {
    render(<SearchTab />);

    const advancedToggle = await screen.findByRole('button', {
      name: /Advanced Retrieval Tuning/i,
    });
    fireEvent.click(advancedToggle);

    const minField = await screen.findByLabelText(/KB Search Min Limit/i);
    fireEvent.change(minField, { target: { value: '120' } });
    fireEvent.blur(minField);

    await waitFor(() => {
      expect(VaultAPI.updateSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          category: 'search',
          updates: expect.objectContaining({
            retrievalTuning: expect.objectContaining({
              kbSearchMinLimit: 120,
              kbSearchMaxLimit: 120,
            }),
          }),
        })
      );
    });
  });
});
