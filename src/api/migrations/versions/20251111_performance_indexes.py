"""Add critical performance indexes for 10x query speedup

Revision ID: 20251111_performance_indexes
Revises: 20251111_upgrade_bge_m3_embeddings
Create Date: 2025-11-11
"""
from alembic import op

revision = "20251111_performance_indexes"
down_revision = "20251111_upgrade_bge_m3_embeddings"
branch_labels = None
depends_on = None


def upgrade() -> None:
    """Add comprehensive performance indexes."""
    
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_files_created_at_desc ON files(created_at DESC)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_files_modified_at_desc ON files(modified_at DESC)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_files_size_bytes ON files(size_bytes)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_files_filename_lower ON files(LOWER(filename))")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_files_mime_created ON files(mime_type, created_at DESC)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_files_extension_created ON files(extension, created_at DESC)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_files_active_recent ON files(created_at DESC) WHERE is_deleted = false")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_files_hot_data ON files(id, filename, mime_type) WHERE created_at > NOW() - INTERVAL '30 days' AND is_deleted = false")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_files_processing_status ON files(processing_status, created_at DESC)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_files_list_covering ON files(created_at DESC, id, filename, mime_type, file_size, modified_at) WHERE is_deleted = false")
    
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_text_content_word_count ON text_content(word_count)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_text_content_char_count ON text_content(char_count)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_text_content_language_file ON text_content(language, file_id)")
    
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_text_embeddings_chunk_lookup ON text_embeddings(text_content_id, chunk_index)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_text_embeddings_created ON text_embeddings(created_at DESC)")
    
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_images_format_dimensions ON images(format, width, height)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_images_has_exif ON images(file_id) WHERE exif_data IS NOT NULL")
    
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tags_name_lower ON tags(LOWER(name))")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tags_created ON tags(created_at DESC)")
    
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_file_tags_file_tag ON file_tags(file_id, tag_id)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_file_tags_tag_file ON file_tags(tag_id, file_id)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_file_tags_created ON file_tags(created_at DESC)")
    
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_search_history_query_hash ON search_history(md5(query))")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_search_history_user_created ON search_history(electric_user_id, created_at DESC)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_search_history_type_created ON search_history(search_type, created_at DESC)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_search_history_slow_queries ON search_history(execution_time_ms DESC, created_at DESC) WHERE execution_time_ms > 1000")
    
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_watch_folders_active_path ON watch_folders(active, path) WHERE active = true")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_watch_folders_last_scan ON watch_folders(last_scan_at DESC NULLS LAST)")
    
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_files_hash_lookup ON files(hash_sha256) WHERE is_deleted = false")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_files_last_accessed ON files(last_accessed_at DESC NULLS LAST)")
    op.execute("CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_images_has_thumbnail ON images(file_id) WHERE thumbnail_path IS NOT NULL")


def downgrade() -> None:
    """Remove performance indexes."""
    
    indexes = [
        "idx_files_created_at_desc", "idx_files_modified_at_desc", "idx_files_size_bytes",
        "idx_files_filename_lower", "idx_files_mime_created", "idx_files_extension_created",
        "idx_files_active_recent", "idx_files_hot_data", "idx_files_processing_status",
        "idx_files_list_covering", "idx_files_hash_lookup", "idx_files_last_accessed",
        "idx_text_content_word_count", "idx_text_content_char_count", "idx_text_content_language_file",
        "idx_text_embeddings_chunk_lookup", "idx_text_embeddings_created",
        "idx_images_format_dimensions", "idx_images_has_exif", "idx_images_has_thumbnail",
        "idx_tags_name_lower", "idx_tags_created",
        "idx_file_tags_file_tag", "idx_file_tags_tag_file", "idx_file_tags_created",
        "idx_search_history_query_hash", "idx_search_history_user_created",
        "idx_search_history_type_created", "idx_search_history_slow_queries",
        "idx_watch_folders_active_path", "idx_watch_folders_last_scan"
    ]
    
    for idx in indexes:
        op.execute(f"DROP INDEX CONCURRENTLY IF EXISTS {idx}")
