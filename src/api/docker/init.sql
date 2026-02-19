-- ================================================================================
-- Vault Backend Database Initialization Script
-- ================================================================================
-- This script initializes PostgreSQL databases for the Vault backend
-- It sets up required extensions, users, and permissions

-- Enable required extensions in the postgres database
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Create electric user and database for ElectricSQL
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = 'electric') THEN
        CREATE USER electric WITH PASSWORD 'electric';
    END IF;
END
$$;

-- Create electric database if it doesn't exist
SELECT 'CREATE DATABASE electric OWNER electric'
WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'electric')\gexec

-- Grant necessary permissions
GRANT ALL PRIVILEGES ON DATABASE vault TO vault;
GRANT ALL PRIVILEGES ON DATABASE electric TO electric;

-- ================================================================================
-- Configure Vault Database
-- ================================================================================
\c vault;

-- Enable extensions in vault database
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS btree_gin;
CREATE EXTENSION IF NOT EXISTS btree_gist;

-- Create basic schema (detailed schema will be managed by Alembic migrations)
CREATE SCHEMA IF NOT EXISTS public;

-- Grant schema permissions
GRANT ALL ON SCHEMA public TO vault;
GRANT USAGE ON SCHEMA public TO vault;
GRANT CREATE ON SCHEMA public TO vault;

-- Set default privileges for future tables
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO vault;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO vault;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON FUNCTIONS TO vault;

-- Configure full-text search
-- Create text search configuration if needed
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_ts_config WHERE cfgname = 'vault_search'
    ) THEN
        CREATE TEXT SEARCH CONFIGURATION vault_search (COPY = pg_catalog.english);
    END IF;
END
$$;

-- Performance optimizations
-- Set work_mem for better query performance with vectors
ALTER DATABASE vault SET work_mem = '64MB';
ALTER DATABASE vault SET maintenance_work_mem = '256MB';
ALTER DATABASE vault SET effective_cache_size = '4GB';

-- Enable parallel query execution
ALTER DATABASE vault SET max_parallel_workers_per_gather = 4;

-- ================================================================================
-- Configure ElectricSQL Database
-- ================================================================================
\c electric;

-- Enable extensions in electric database
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Enable logical replication (required for ElectricSQL)
-- Note: This requires appropriate postgresql.conf settings
-- wal_level = logical
-- max_replication_slots >= 4
-- max_wal_senders >= 4

-- Create schema
CREATE SCHEMA IF NOT EXISTS public;

-- Grant schema permissions
GRANT ALL ON SCHEMA public TO electric;
GRANT USAGE ON SCHEMA public TO electric;
GRANT CREATE ON SCHEMA public TO electric;

-- Set default privileges
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO electric;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO electric;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON FUNCTIONS TO electric;

-- Grant replication permissions to electric user
ALTER USER electric WITH REPLICATION;

-- ================================================================================
-- Create monitoring views (optional, for production monitoring)
-- ================================================================================
\c vault;

-- View for checking index health
CREATE OR REPLACE VIEW index_health AS
SELECT
    schemaname,
    tablename,
    indexname,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch,
    pg_size_pretty(pg_relation_size(indexrelid)) AS index_size
FROM pg_stat_user_indexes
ORDER BY idx_scan;

-- View for checking table sizes
CREATE OR REPLACE VIEW table_sizes AS
SELECT
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS total_size,
    pg_size_pretty(pg_relation_size(schemaname||'.'||tablename)) AS table_size,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename) - pg_relation_size(schemaname||'.'||tablename)) AS indexes_size
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;

-- Grant select on views
GRANT SELECT ON index_health TO vault;
GRANT SELECT ON table_sizes TO vault;

-- ================================================================================
-- Initialization Complete
-- ================================================================================
-- Database setup is complete. The application will handle schema migrations
-- using Alembic.
