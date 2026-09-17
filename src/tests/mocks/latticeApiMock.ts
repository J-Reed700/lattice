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
      excludePatterns: [],
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
        sufficiencyMinTopScore: 0.35,
        sufficiencyMinTermCoverage: 0.4,
        sufficiencyRetryEnabled: true,
        overlapMinHitsForMultiTerm: 2,
        docSupportMultiHitRatioFactor: 0.35,
        docSupportSingleHitRatioFactor: 0.65,
        docSupportMultiHitRatioMin: 0.18,
        docSupportMultiHitRatioMax: 0.4,
        docSupportSingleHitRatioMin: 0.3,
        docSupportSingleHitRatioMax: 0.55,
      },
      vectorIndexCompression: { mode: 'none', dims: 512, quantization: 'i8' },
      embeddingStrategy: 'chunk_first',
      summaryIndexEnabled: false,
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
          "You are Lattice, a precise research assistant. Use the user's documents when available. Cite sources using [#]. Never fabricate document IDs.",
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
    clearIndexingFailure: vi.fn().mockResolvedValue({ ok: true, data: undefined }),
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
    warmUpActiveUtilityModel: vi.fn().mockResolvedValue({ ok: true, data: undefined }),
    getModelInfo: vi.fn().mockResolvedValue({
      name: 'test-model',
      size: 1000000,
      status: 'ready'
    }),
    listModels: vi.fn().mockResolvedValue([]),
    getModelDownloadPath: vi
      .fn()
      .mockResolvedValue({ ok: true, data: '/tmp/lattice/models' }),

    // Q&A operations
    askQuestion: vi.fn().mockResolvedValue({
      answer: 'mock answer',
      sources: []
    }),
    askQuestionStream: vi.fn().mockResolvedValue({
      ok: true,
      data: {
        answer: 'mock answer',
        sources: [],
      },
    }),

    // Settings operations
    getSettings: vi.fn().mockResolvedValue(mockSettings),
    updateSettings: vi.fn().mockResolvedValue(mockSettings),
    testLlamaCppConnection: vi.fn().mockResolvedValue({ ok: true, data: { endpoint: '/v1/chat/completions', models: ['test-model'] } }),
  testOllamaConnection: vi.fn().mockResolvedValue({
      ok: true,
      data: {
        endpoint: 'http://localhost:11434',
        models: [],
      },
    }),

    // Tag operations
    getDocumentTags: vi.fn().mockResolvedValue({
      ok: true,
      data: { documentId: 'test-document', tags: [] },
    }),
    listAllTags: vi.fn().mockResolvedValue({
      ok: true,
      data: { tags: [] },
    }),
    generateTagsForDocument: vi.fn().mockResolvedValue({ ok: true, data: [] }),
    applyTags: vi.fn().mockResolvedValue({ ok: true, data: [] }),
    removeTagFromDocument: vi.fn().mockResolvedValue({ ok: true, data: undefined }),

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
    getConversationMessages: vi.fn().mockResolvedValue({
      ok: true,
      data: { messages: [], total: 0 },
    }),

    // Workspace note operations
    listWorkspaceNotes: vi.fn().mockResolvedValue({
      ok: true,
      data: { notes: [] },
    }),
    createWorkspaceNote: vi.fn().mockResolvedValue({
      ok: true,
      data: {
        id: 'test-note',
        title: 'Test note',
        content: '',
        linkedDocumentIds: [],
        linkedConversationIds: [],
        highlights: [],
        stickyNotes: [],
        conversationSnapshots: [],
        createdAt: '2026-01-01T00:00:00.000Z',
        updatedAt: '2026-01-01T00:00:00.000Z',
      },
    }),
    updateWorkspaceNote: vi.fn().mockImplementation(async (note) => ({
      ok: true,
      data: note,
    })),

    // Branching, starters, and passage references
    truncateConversationAfter: vi.fn().mockResolvedValue({
      ok: true,
      data: { conversationId: 'test-conversation', deletedCount: 0, messages: [] },
    }),
    forkConversation: vi.fn().mockResolvedValue({
      ok: true,
      data: {
        conversation: {
          id: 'test-branch',
          title: 'Test · branch',
          updatedAt: '2026-01-01T00:00:00.000Z',
        },
        copiedMessageCount: 0,
      },
    }),
    regenerateResponse: vi.fn().mockResolvedValue({
      ok: true,
      data: { conversationId: 'test-conversation', messages: [], contextUsed: 0 },
    }),
    compactConversation: vi.fn().mockResolvedValue({
      ok: true,
      data: {
        compaction: {
          id: 'test-compaction',
          conversationId: 'test-conversation',
          summaryText: 'Summary of the older messages.',
          upToMessageId: 'm2',
          originalMessageCount: 4,
          originalTokens: 1200,
          summaryTokens: 150,
          compressionRatio: 0.125,
          createdAt: '2026-01-01T00:00:00.000Z',
        },
      },
    }),
    generateChatStarters: vi.fn().mockResolvedValue({
      ok: true,
      data: {
        fingerprint: 'test',
        generatedAt: '2026-01-01T00:00:00.000Z',
        starters: [],
        documentCount: 0,
      },
    }),
    createPassageReference: vi.fn().mockResolvedValue({
      ok: true,
      data: {
        id: 'test-reference',
        documentId: 'test-document',
        chunkId: null,
        filePath: '/vault/test.md',
        fileName: 'test.md',
        locator: null,
        text: 'excerpt',
        title: null,
        note: null,
        createdAt: '2026-01-01T00:00:00.000Z',
      },
    }),
    listPassageReferences: vi.fn().mockResolvedValue({ ok: true, data: [] }),

    // Off-device backup (encrypted archive).
    // Unconfigured by default: the block renders its setup CTA and nothing
    // else, so tests that are not about archives are unaffected.
    getArchiveStatus: vi.fn().mockResolvedValue({
      ok: true,
      data: {
        configured: false,
        destination: null,
        destinationProvider: null,
        destinationMissing: false,
        keepCount: 5,
        hasPassphrase: false,
        recoveryConfirmed: false,
        lastSuccess: null,
        lastError: null,
        dataDirCloudProvider: null,
        archives: [],
      },
    }),
    beginArchiveSetup: vi.fn().mockResolvedValue({
      ok: true,
      data: { recoveryWords: [], confirmIndices: [] },
    }),
    confirmArchiveSetup: vi
      .fn()
      .mockResolvedValue({ ok: false, error: 'not configured in this test' }),
    chooseArchiveDestination: vi
      .fn()
      .mockResolvedValue({ ok: false, error: 'not configured in this test' }),
    setArchiveKeepCount: vi
      .fn()
      .mockResolvedValue({ ok: false, error: 'not configured in this test' }),
    setArchivePassphrase: vi
      .fn()
      .mockResolvedValue({ ok: false, error: 'not configured in this test' }),
    rotateRecoveryCode: vi.fn().mockResolvedValue({
      ok: true,
      data: { recoveryWords: [], confirmIndices: [] },
    }),
    disableArchive: vi
      .fn()
      .mockResolvedValue({ ok: false, error: 'not configured in this test' }),
    createArchiveNow: vi
      .fn()
      .mockResolvedValue({ ok: false, error: 'not configured in this test' }),
    restoreArchive: vi.fn().mockResolvedValue({
      ok: true,
      data: {
        outcome: 'cancelled',
        message: null,
        restartRequired: false,
        reembedRequired: false,
        vaultRestoredTo: null,
        filesRestored: 0,
      },
    }),

    // Transcription
    transcribeFile: vi.fn().mockResolvedValue({
      ok: true,
      data: {
        filePath: '/vault/voice-memo.m4a',
        language: 'en',
        durationMs: 0,
        segments: [],
        text: '',
      },
    }),
    getTranscriptionStatus: vi
      .fn()
      .mockResolvedValue({ ok: true, data: { modelReady: false } }),

    __reset: vi.fn(() => {
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
