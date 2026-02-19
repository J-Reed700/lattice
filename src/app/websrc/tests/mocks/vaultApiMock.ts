import { vi } from 'vitest';

/**
 * Mock implementation of VaultAPI for testing
 */
export function createVaultAPIMock() {
  const mockSettings = {
    indexing: {
      chunkSize: 800,
      chunkOverlap: 120,
      batchSize: 100,
      autoIndexNewFiles: true,
      fileTypes: [],
      excludedPaths: [],
    },
    search: {
      maxResults: 10,
      similarityThreshold: 0.7,
      enableReranking: false,
      hybridSearchAlpha: 0.5,
      retrievalTuning: {
        kbSearchMinLimit: 16,
        kbSearchMaxLimit: 48,
        docShortlistCandidateMin: 48,
        docShortlistCandidateMax: 192,
        docShortlistDocMin: 10,
        docShortlistDocMax: 32,
        shortlistGateMinCandidates: 8,
        shortlistGateMinDocs: 4,
        wikiSearchMaxResults: 5,
        wikiSnippetMaxChars: 1200,
        wikiContextLimit: 3,
        webSearchMaxResults: 5,
        webSnippetMaxChars: 1200,
        deepResearchDepth: 3,
        deepResearchBranchQueries: 3,
        externalSearchMaxWikiTerms: 8,
        externalSearchMaxWebTerms: 10,
        externalSearchQueryMaxChars: 1200,
        rerankMaxCandidates: 48,
        rerankQueryMaxChars: 6000,
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
      model: 'llama3.2:latest',
      temperature: 0.7,
      topP: 0.9,
      topK: 40,
      repeatPenalty: 1.1,
      maxTokens: 131072,
      contextWindow: 131072,
      ollamaUrl: 'http://localhost:11434',
      ollamaAuthHeaderName: '',
      ollamaAuthHeaderValue: '',
      timeoutSeconds: 30,
      streamResponses: true,
      prompts: {
        systemPrompt:
          "You are Recall, a precise research assistant. Use the user's documents when available. Cite sources using [#]. Never fabricate document IDs.",
        greetingPromptTemplate:
          'The user greeted you: "{question}". Reply briefly, warmly, and offer help with their knowledge base.',
        ragPromptTemplate:
          "Answer the user's question using only the provided context. Cite sources using [#]. If you need to call get_document, use the exact Document ID shown in the context.\n\nContext:\n{context}\n\nQuestion: {question}\n\nAnswer:",
        noContextPromptTemplate:
          'The user asked: "{question}"\n\nNo relevant documents were found in their knowledge base. Respond politely and ask if they\'d like to upload or specify a document.',
        toolFollowupPromptTemplate:
          'Tool results have been added to the context. Use them to answer the user\'s question. If excerpts are provided, quote them briefly and avoid repetition.\n\nQuestion: {question}\n{previous_response}\nAnswer:',
      },
      verification: {
        enabled: true,
      },
      toolOutput: {
        maxChars: 50000,
        excerptChars: 4000,
        maxResults: 5,
        highlightTermsMax: 8,
        templates: {
          defaultTemplate: 'Tool output:\n{json}',
          getDocumentTemplate:
            'Document ID: {document_id}\nTitle: {title}\nChunks: {chunk_count}\nExcerpt: {excerpt}{truncated}',
          semanticSearchTemplate:
            'Search results (showing {shown} of {total_found}):\n{results}',
        },
      },
      router: {
        enabled: true,
        model: 'phi-3.5-mini-instruct-q4_k_m',
        timeoutMs: 350,
        maxTokens: 120,
        temperature: 0.1,
        ambiguityThreshold: 0.62,
        preferLastDocument: true,
        promptTemplate:
          'You are a routing assistant for a knowledge base chat.\nReturn ONLY valid JSON with the fields:\n- action: "use_last_document" | "new_search" | "clarify"\n- confidence: number from 0 to 1\n- clarify_question: string or null\n- rationale: short string\n\nUser query: {query}\nHas recent document: {has_recent_document}\nRecent document title: {recent_document_title}\nRecent document id: {recent_document_id}\n\nDecide whether the user is referring to the recent document or wants a new search. If uncertain, choose "clarify".\nJSON:',
        clarifyPromptTemplate:
          'Are you asking about the previous document ({recent_document_title}), or do you want me to search for a different document?',
      },
      externalModelDirectories: [],
      customTools: [],
    },
    ui: {
      theme: 'system',
      fontSize: 14,
      showPreview: true,
      resultsPerPage: 10,
      enableAnimations: true,
    },
    sync: {
      syncEnabled: false,
      syncUrl: '',
      syncIntervalMinutes: 30,
      autoSync: false,
      syncOnStartup: false,
    },
    backup: {
      autoBackupEnabled: false,
      backupFrequency: 'daily',
      backupRetentionDays: 30,
      backupPath: '',
      compressBackups: true,
    },
  };

  const api = {
    // File operations
    indexFile: vi.fn().mockResolvedValue({ success: true }),
    indexDirectory: vi.fn().mockResolvedValue({ success: true }),
    deleteDocument: vi.fn().mockResolvedValue({ success: true }),
    getFileContent: vi.fn().mockResolvedValue({ content: 'mock content' }),

    // Search operations
    search: vi.fn().mockResolvedValue({
      results: [],
      total: 0,
      facets: {}
    }),
    hybridSearch: vi.fn().mockResolvedValue({
      results: [],
      total: 0,
      facets: {}
    }),

    // Model operations
    downloadModel: vi.fn().mockResolvedValue({ success: true }),
    warmUpActiveChatModel: vi.fn().mockResolvedValue({ ok: true, data: undefined }),
    getModelInfo: vi.fn().mockResolvedValue({
      name: 'test-model',
      size: 1000000,
      status: 'ready'
    }),
    listModels: vi.fn().mockResolvedValue([]),

    // Q&A operations
    askQuestion: vi.fn().mockResolvedValue({
      answer: 'mock answer',
      sources: []
    }),

    // Settings operations
    getSettings: vi.fn().mockResolvedValue(mockSettings),
    updateSettings: vi.fn().mockResolvedValue(mockSettings),
    getConfig: vi.fn().mockResolvedValue({
      indexedPaths: [],
      excludePatterns: [],
      autoIndex: true,
      ollamaEndpoint: 'http://localhost:11434',
      ollamaModel: 'llama3.1:8b',
    }),

    // Stats operations
    getStats: vi.fn().mockResolvedValue({
      totalDocuments: 0,
      totalChunks: 0,
      indexSize: 0
    }),

    // Progress operations
    getProgress: vi.fn().mockResolvedValue({
      status: 'idle',
      processedFiles: 0,
      totalFiles: 0,
      percentage: 0
    }),
    cancelIndexing: vi.fn().mockResolvedValue({ success: true }),

    // Conversation operations
    startConversation: vi.fn().mockResolvedValue({
      conversationId: 'test-id'
    }),
    sendMessage: vi.fn().mockResolvedValue({
      response: 'mock response',
      sources: []
    }),

    // Mock utilities
    __reset: vi.fn(() => {
      // Reset all mocks
      Object.values(api).forEach((value) => {
        const maybeMock = value as { mockReset?: () => void };
        if (typeof value === 'function' && maybeMock.mockReset) {
          maybeMock.mockReset();
        }
      });
    }),
  };

  return api;
}

export default createVaultAPIMock;
