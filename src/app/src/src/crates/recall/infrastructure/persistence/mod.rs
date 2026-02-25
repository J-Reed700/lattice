//! Persistence Infrastructure
//!
//! This module contains database access and persistence implementations:
//! - **Database**: SQLite connection management and pooling
//! - **Migrations**: Database schema migrations
//! - **Repositories**: Repository pattern implementations for data access
//! - **Models**: SQLAlchemy-style ORM models for database tables
//! - **Mappers**: Domain entity ↔ DB model conversion (Clean Architecture boundary)
//!
//! # Clean Architecture Layer
//!
//! The **mapper layer** is critical for Clean Architecture:
//! - **Domain entities** (rich models) stay pure - no DB concerns
//! - **DB models** (anemic DTOs) handle persistence
//! - **Mappers** translate between the two
//!
//! This ensures the domain layer has ZERO infrastructure dependencies.
//!
//! # Migration Status
//! - [x] mappers/ - Domain ↔ DB model conversion layer (Phase 1)
//! - [x] database.rs - Moved from `db/database.rs`
//! - [ ] Migrations - Will move from `db/migrations/`
//! - [ ] Repositories - Will move from `db/repositories/`
//! - [ ] Models - Will move from `db/models/`
//!
//! # Dependencies
//! - Uses: Domain entities, Application repository ports
//! - Provides: Concrete repository implementations

pub mod backup_adapter;
pub mod database;
pub mod download_repository;
pub mod helpers;
pub mod mappers;
pub mod migrations;
pub mod repositories;

pub use backup_adapter::BackupAdapter;
pub use download_repository::{DownloadRepository, SqliteDownloadRepository};
pub use helpers::query_indexed_directories;
