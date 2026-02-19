import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { TagManager } from '../../components/TagManager/TagManager';
import { VaultAPI } from '@/lib/api';

/**
 * Tagging Workflow Integration Tests
 *
 * NOTE: Migrated to DDD tag commands (Phase 1 - Frontend Migration):
 * - get_document_tags → get_tags_for_document
 * - generate_tags → generate_tags_for_document
 * - All other tag commands remain the same name (DDD implementations)
 */
describe('Tagging Workflow', () => {
  const mockGetDocumentTags = vi.mocked(VaultAPI.getDocumentTags);
  const mockListAllTags = vi.mocked(VaultAPI.listAllTags);
  const mockGenerateTagsForDocument = vi.mocked(VaultAPI.generateTagsForDocument);
  const mockApplyTags = vi.mocked(VaultAPI.applyTags);
  const mockRemoveTagFromDocument = vi.mocked(VaultAPI.removeTagFromDocument);

  beforeEach(() => {
    vi.clearAllMocks();
    mockGetDocumentTags.mockResolvedValue({ ok: true, data: { tags: [], documentId: 'doc' } });
    mockListAllTags.mockResolvedValue({ ok: true, data: { tags: [] } });
    mockGenerateTagsForDocument.mockResolvedValue({ ok: true, data: [] });
    mockApplyTags.mockResolvedValue({ ok: true, data: [] });
    mockRemoveTagFromDocument.mockResolvedValue({ ok: true, data: undefined });
  });

  it('auto-generates tags and displays them', async () => {
    const user = userEvent.setup();
    const documentId = 'test-doc-123';

    mockGetDocumentTags.mockResolvedValueOnce({ ok: true, data: { tags: [], documentId } });
    mockListAllTags.mockResolvedValueOnce({
      ok: true,
      data: {
        tags: [
          { id: '1', name: 'machine-learning', color: '#3b82f6', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
          { id: '2', name: 'ai', color: '#8b5cf6', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
        ]
      }
    });
    mockGenerateTagsForDocument.mockResolvedValueOnce({
      ok: true,
      data: ['machine-learning', 'neural-networks', 'deep-learning'],
    });
    mockApplyTags.mockResolvedValueOnce({
      ok: true,
      data: [
        { id: '1', name: 'machine-learning', color: '#3b82f6', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
        { id: '2', name: 'neural-networks', color: '#10b981', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
        { id: '3', name: 'deep-learning', color: '#f59e0b', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
      ],
    });

    render(<TagManager documentId={documentId} />);

    expect(screen.getByText('Tags')).toBeInTheDocument();
    expect(screen.getByText('Auto-generate')).toBeInTheDocument();

    const autoGenerateBtn = screen.getByText('Auto-generate');
    await user.click(autoGenerateBtn);

    await waitFor(() => {
      expect(mockGenerateTagsForDocument).toHaveBeenCalledWith(documentId);
    });

    await waitFor(() => {
      expect(mockApplyTags).toHaveBeenCalledWith(documentId, ['machine-learning', 'neural-networks', 'deep-learning']);
    });

    await waitFor(() => {
      expect(screen.getByText('machine-learning')).toBeInTheDocument();
      expect(screen.getByText('neural-networks')).toBeInTheDocument();
      expect(screen.getByText('deep-learning')).toBeInTheDocument();
    });
  });

  it('allows manual tag addition', async () => {
    const user = userEvent.setup();
    const documentId = 'test-doc-456';

    mockGetDocumentTags.mockResolvedValueOnce({ ok: true, data: { tags: [], documentId } });
    mockListAllTags.mockResolvedValueOnce({ ok: true, data: { tags: [] } });
    mockApplyTags.mockResolvedValueOnce({
      ok: true,
      data: [
        { id: '1', name: 'custom-tag', color: '#6366f1', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
      ],
    });

    render(<TagManager documentId={documentId} />);

    const input = screen.getByPlaceholderText('Add tag...');
    await user.type(input, 'custom-tag');

    const addBtn = screen.getByText('Add');
    await user.click(addBtn);

    await waitFor(() => {
      expect(mockApplyTags).toHaveBeenCalledWith(documentId, ['custom-tag']);
    });
  });

  it('handles tag autocomplete', async () => {
    const user = userEvent.setup();
    const documentId = 'test-doc-789';

    mockGetDocumentTags.mockResolvedValueOnce({ ok: true, data: { tags: [], documentId } });
    mockListAllTags.mockResolvedValueOnce({
      ok: true,
      data: {
        tags: [
          { id: '1', name: 'machine-learning', color: '#3b82f6', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
          { id: '2', name: 'machine-vision', color: '#10b981', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
          { id: '3', name: 'rust', color: '#ef4444', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
        ]
      }
    });
    mockApplyTags.mockResolvedValueOnce({
      ok: true,
      data: [
        { id: '1', name: 'machine-learning', color: '#3b82f6', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
      ],
    });

    render(<TagManager documentId={documentId} />);

    await waitFor(() => {
      expect(screen.getByPlaceholderText('Add tag...')).toBeInTheDocument();
    });

    const input = screen.getByPlaceholderText('Add tag...');
    await user.type(input, 'mach');

    await waitFor(() => {
      expect(screen.getByText('machine-learning')).toBeInTheDocument();
      expect(screen.getByText('machine-vision')).toBeInTheDocument();
    });
  });

  it('removes tags', async () => {
    const user = userEvent.setup();
    const documentId = 'test-doc-remove';

    mockGetDocumentTags.mockResolvedValueOnce({
      ok: true,
      data: {
        tags: [
          { id: '1', name: 'old-tag', color: '#6366f1', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
        ],
        documentId,
      }
    });
    mockListAllTags.mockResolvedValueOnce({ ok: true, data: { tags: [] } });
    mockRemoveTagFromDocument.mockResolvedValueOnce({ ok: true, data: undefined });

    render(<TagManager documentId={documentId} />);

    await waitFor(() => {
      expect(screen.getByText('old-tag')).toBeInTheDocument();
    });

    const removeBtn = screen.getByRole('button', { name: /remove/i });
    if (removeBtn) {
      await user.click(removeBtn);

      await waitFor(() => {
        expect(mockRemoveTagFromDocument).toHaveBeenCalledWith({
          documentId,
          tagId: '1',
        });
      });
    }
  });

  it('displays error when generation fails', async () => {
    const user = userEvent.setup();
    const documentId = 'test-doc-error';

    mockGetDocumentTags.mockResolvedValueOnce({ ok: true, data: { tags: [], documentId } });
    mockListAllTags.mockResolvedValueOnce({ ok: true, data: { tags: [] } });
    mockGenerateTagsForDocument.mockResolvedValueOnce({
      ok: false,
      error: 'Python backend not available',
    });

    render(<TagManager documentId={documentId} />);

    const autoGenerateBtn = screen.getByText('Auto-generate');
    await user.click(autoGenerateBtn);

    await waitFor(() => {
      expect(screen.getByText(/Python backend not available/i)).toBeInTheDocument();
    });
  });

  it('filters already applied tags from autocomplete', async () => {
    const user = userEvent.setup();
    const documentId = 'test-doc-filter';

    mockGetDocumentTags.mockResolvedValueOnce({
      ok: true,
      data: {
        tags: [
          { id: '1', name: 'machine-learning', color: '#3b82f6', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
        ],
        documentId,
      }
    });
    mockListAllTags.mockResolvedValueOnce({
      ok: true,
      data: {
        tags: [
          { id: '1', name: 'machine-learning', color: '#3b82f6', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
          { id: '2', name: 'machine-vision', color: '#10b981', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
          { id: '3', name: 'ai', color: '#ef4444', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
        ]
      }
    });

    render(<TagManager documentId={documentId} />);

    await waitFor(() => {
      expect(screen.getByText('machine-learning')).toBeInTheDocument();
    });

    const input = screen.getByPlaceholderText('Add tag...');
    await user.type(input, 'machine');

    await waitFor(() => {
      expect(screen.getByText('machine-vision')).toBeInTheDocument();
    });

    const machineVisionBtn = screen.getByText('machine-vision');
    expect(machineVisionBtn).toBeInTheDocument();
  });

  it('handles tag input with Enter key', async () => {
    const user = userEvent.setup();
    const documentId = 'test-doc-enter';

    mockGetDocumentTags.mockResolvedValueOnce({ ok: true, data: { tags: [], documentId } });
    mockListAllTags.mockResolvedValueOnce({ ok: true, data: { tags: [] } });
    mockApplyTags.mockResolvedValueOnce({
      ok: true,
      data: [
        { id: '1', name: 'new-tag', color: '#6366f1', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
      ],
    });

    render(<TagManager documentId={documentId} />);

    const input = screen.getByPlaceholderText('Add tag...');
    await user.type(input, 'new-tag{Enter}');

    await waitFor(() => {
      expect(mockApplyTags).toHaveBeenCalledWith(documentId, ['new-tag']);
    });
  });

  it('clears input after adding tag', async () => {
    const user = userEvent.setup();
    const documentId = 'test-doc-clear';

    mockGetDocumentTags.mockResolvedValueOnce({ ok: true, data: { tags: [], documentId } });
    mockListAllTags.mockResolvedValueOnce({ ok: true, data: { tags: [] } });
    mockApplyTags.mockResolvedValueOnce({
      ok: true,
      data: [
        { id: '1', name: 'test-tag', color: '#6366f1', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
      ],
    });

    render(<TagManager documentId={documentId} />);

    const input = screen.getByPlaceholderText('Add tag...') as HTMLInputElement;
    await user.type(input, 'test-tag');

    const addBtn = screen.getByText('Add');
    await user.click(addBtn);

    await waitFor(() => {
      expect(input.value).toBe('');
    });
  });

  it('handles escape key in tag input', async () => {
    const user = userEvent.setup();
    const documentId = 'test-doc-esc';

    mockGetDocumentTags.mockResolvedValueOnce({ ok: true, data: { tags: [], documentId } });
    mockListAllTags.mockResolvedValueOnce({
      ok: true,
      data: {
        tags: [
          { id: '1', name: 'existing-tag', color: '#3b82f6', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
        ]
      }
    });

    render(<TagManager documentId={documentId} />);

    const input = screen.getByPlaceholderText('Add tag...');
    await user.type(input, 'existing{Escape}');

    const dropdown = screen.queryByText('existing-tag');
    expect(dropdown).not.toBeInTheDocument();
  });

  it('handles large number of tags', async () => {
    const documentId = 'test-doc-many';

    const manyTags = Array.from({ length: 10 }, (_, i) => ({
      id: `${i}`,
      name: `tag-${i}`,
      color: '#3b82f6',
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    }));

    mockGetDocumentTags.mockResolvedValueOnce({ ok: true, data: { tags: manyTags.slice(0, 5), documentId } });
    mockListAllTags.mockResolvedValueOnce({ ok: true, data: { tags: manyTags } });

    render(<TagManager documentId={documentId} />);

    await waitFor(() => {
      expect(screen.getByText('tag-0')).toBeInTheDocument();
    });

    for (let i = 0; i < 5; i++) {
      expect(screen.getByText(`tag-${i}`)).toBeInTheDocument();
    }
  });

  it('displays loading state when fetching tags', async () => {
    const documentId = 'test-doc-loading';

    mockGetDocumentTags.mockImplementationOnce(
      () => new Promise(resolve => setTimeout(() => resolve({ ok: true, data: { tags: [], documentId } }), 100))
    );
    mockListAllTags.mockResolvedValueOnce({ ok: true, data: { tags: [] } });

    render(<TagManager documentId={documentId} />);

    expect(screen.getByText(/Loading tags/i)).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.queryByText(/Loading tags/i)).not.toBeInTheDocument();
    });
  });

  it('prevents adding empty tags', async () => {
    const user = userEvent.setup();
    const documentId = 'test-doc-empty';

    mockGetDocumentTags.mockResolvedValueOnce({ ok: true, data: { tags: [], documentId } });
    mockListAllTags.mockResolvedValueOnce({ ok: true, data: { tags: [] } });

    render(<TagManager documentId={documentId} />);

    const addBtn = screen.getByText('Add');
    expect(addBtn).toBeDisabled();

    const input = screen.getByPlaceholderText('Add tag...');
    await user.type(input, '   ');

    expect(addBtn).toBeDisabled();
  });

  it('normalizes tag names to lowercase', async () => {
    const user = userEvent.setup();
    const documentId = 'test-doc-case';

    mockGetDocumentTags.mockResolvedValueOnce({ ok: true, data: { tags: [], documentId } });
    mockListAllTags.mockResolvedValueOnce({ ok: true, data: { tags: [] } });
    mockApplyTags.mockImplementationOnce((_docId, tagNames) => {
      expect(tagNames[0]).toBe(tagNames[0].toLowerCase());
      return Promise.resolve({
        ok: true,
        data: [
          { id: '1', name: 'capitalized-tag', color: '#6366f1', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() },
        ],
      });
    });

    render(<TagManager documentId={documentId} />);

    const input = screen.getByPlaceholderText('Add tag...');
    await user.type(input, 'CAPITALIZED-TAG');

    const addBtn = screen.getByText('Add');
    await user.click(addBtn);

    await waitFor(() => {
      expect(mockApplyTags).toHaveBeenCalledWith(documentId, ['capitalized-tag']);
    });
  });
});
