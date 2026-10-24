//! Persistence Infrastructure
//!
//! Database access and persistence implementations:
//! - **Database**: SQLite connection management and pooling
//! - **Repositories**: Repository implementations for data access
//! - **Models**: Database models for SQLite tables
//! - **Mappers**: Domain entity ↔ DB model conversion
//!
//! The mapper layer isolates domain entities from database persistence details,
//! translating between rich domain entities and database models.

pub mod database;
pub mod helpers;
pub mod mappers;
pub mod repositories;

pub use helpers::query_indexed_directories;
