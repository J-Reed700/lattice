use crate::shared::error::Result;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePool, Row};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub file_path: String,
    pub file_name: String,
    pub file_type: Option<String>,
    pub size_bytes: i64,
    pub modified_at: String,
    pub indexed_at: String,
    pub checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub document_id: String,
    pub content: String,
    pub embedding: Vec<f32>,
    pub chunk_index: i32,
}

pub struct DatabaseService {
    pool: SqlitePool,
}

impl DatabaseService {
    pub async fn new(db_path: &str) -> Result<Self> {
        let pool = SqlitePool::connect(&format!("sqlite://{}", db_path)).await?;
        Ok(Self { pool })
    }

    pub async fn insert_document(&self, doc: &Document) -> Result<String> {
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO documents (id, file_path, file_name, file_type, size_bytes, modified_at, checksum)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(&doc.file_path)
        .bind(&doc.file_name)
        .bind(&doc.file_type)
        .bind(doc.size_bytes)
        .bind(&doc.modified_at)
        .bind(&doc.checksum)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    pub async fn get_document(&self, id: &str) -> Result<Option<Document>> {
        let row = sqlx::query(
            "SELECT id, file_path, file_name, file_type, size_bytes, modified_at, indexed_at, checksum
             FROM documents WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Document {
            id: r.get("id"),
            file_path: r.get("file_path"),
            file_name: r.get("file_name"),
            file_type: r.get("file_type"),
            size_bytes: r.get("size_bytes"),
            modified_at: r.get("modified_at"),
            indexed_at: r.get("indexed_at"),
            checksum: r.get("checksum"),
        }))
    }
}
