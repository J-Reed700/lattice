//! Persistence Infrastructure
//!
//! This module contains database access and persistence implementations:
//! - **Database**: SQLite connection management and pooling
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
//! # Dependencies
//! - Uses: Domain entities, Application repository ports
//! - Provides: Concrete repository implementations

pub mod database;
pub mod helpers;
pub mod mappers;
pub mod repositories;

pub use crate::features::download::download_repository::{
    DownloadRepository, SqliteDownloadRepository,
};
pub use helpers::query_indexed_directories;
