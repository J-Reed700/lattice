use super::*;

#[derive(Debug, Clone, sqlx::FromRow)]
struct DocumentSpaceMembershipRow {
    space_id: String,
    space_name: String,
    is_archived: i64,
    created_at: Option<String>,
}
fn validate_space_role(role: &str) -> Result<&str, AppError> {
    match role.trim().to_lowercase().as_str() {
        "owner" => Ok("owner"),
        "editor" => Ok("editor"),
        "viewer" => Ok("viewer"),
        _ => Err(AppError::InvalidInput(
            "Role must be one of: owner, editor, viewer".to_string(),
        )),
    }
}

impl ConversationRepository {
    pub async fn list_conversation_space_members(
        &self,
        space_id: String,
    ) -> Result<Vec<ConversationSpaceMemberDto>, AppError> {
        let members = sqlx::query_as::<_, ConversationSpaceMemberDto>(
            r#"
        SELECT
            sm.space_id AS space_id,
            sm.member_id AS member_id,
            cp.display_name AS display_name,
            cp.email AS email,
            cp.avatar_url AS avatar_url,
            sm.role AS role,
            sm.created_at AS created_at,
            sm.updated_at AS updated_at
        FROM conversation_space_members sm
        INNER JOIN collaborator_profiles cp ON cp.id = sm.member_id
        WHERE sm.space_id = ?
        ORDER BY
            CASE sm.role
                WHEN 'owner' THEN 0
                WHEN 'editor' THEN 1
                ELSE 2
            END,
            cp.display_name COLLATE NOCASE ASC
        "#,
        )
        .bind(&space_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to list conversation space members: {}", e))
        })?;

        Ok(members)
    }

    pub async fn upsert_conversation_space_member(
        &self,
        request: UpsertConversationSpaceMemberRequestDto,
    ) -> Result<ConversationSpaceMemberDto, AppError> {
        let role = validate_space_role(&request.role)?;

        let space_exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM conversation_spaces WHERE id = ?")
                .bind(&request.space_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| {
                    AppError::Database(format!(
                        "Failed to verify space before upserting member: {}",
                        e
                    ))
                })?;
        if space_exists == 0 {
            return Err(AppError::NotFound(format!(
                "Space not found: {}",
                request.space_id
            )));
        }

        let now = Utc::now().to_rfc3339();
        let display_name = request
            .display_name
            .clone()
            .unwrap_or_else(|| request.member_id.clone());

        let mut tx = self.pool.begin().await.map_err(|e| {
            AppError::Database(format!(
                "Failed to begin transaction while upserting space member: {}",
                e
            ))
        })?;

        sqlx::query(
            r#"
        INSERT INTO collaborator_profiles (
            id, display_name, email, avatar_url, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            display_name = excluded.display_name,
            email = COALESCE(excluded.email, collaborator_profiles.email),
            avatar_url = COALESCE(excluded.avatar_url, collaborator_profiles.avatar_url),
            updated_at = excluded.updated_at
        "#,
        )
        .bind(&request.member_id)
        .bind(display_name)
        .bind(request.email)
        .bind(request.avatar_url)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to upsert collaborator profile: {}", e)))?;

        sqlx::query(
            r#"
        INSERT INTO conversation_space_members (
            space_id, member_id, role, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(space_id, member_id) DO UPDATE SET
            role = excluded.role,
            updated_at = excluded.updated_at
        "#,
        )
        .bind(&request.space_id)
        .bind(&request.member_id)
        .bind(role)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to upsert conversation space member: {}", e))
        })?;

        tx.commit().await.map_err(|e| {
            AppError::Database(format!(
                "Failed to commit conversation space member transaction: {}",
                e
            ))
        })?;

        let member = sqlx::query_as::<_, ConversationSpaceMemberDto>(
            r#"
        SELECT
            sm.space_id AS space_id,
            sm.member_id AS member_id,
            cp.display_name AS display_name,
            cp.email AS email,
            cp.avatar_url AS avatar_url,
            sm.role AS role,
            sm.created_at AS created_at,
            sm.updated_at AS updated_at
        FROM conversation_space_members sm
        INNER JOIN collaborator_profiles cp ON cp.id = sm.member_id
        WHERE sm.space_id = ? AND sm.member_id = ?
        LIMIT 1
        "#,
        )
        .bind(&request.space_id)
        .bind(&request.member_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to fetch upserted conversation space member: {}",
                e
            ))
        })?;

        Ok(member)
    }

    pub async fn remove_conversation_space_member(
        &self,
        request: RemoveConversationSpaceMemberRequestDto,
    ) -> Result<RenameConversationResponseDto, AppError> {
        let current_role = sqlx::query_scalar::<_, String>(
            r#"
        SELECT role
        FROM conversation_space_members
        WHERE space_id = ? AND member_id = ?
        LIMIT 1
        "#,
        )
        .bind(&request.space_id)
        .bind(&request.member_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to verify conversation space member: {}", e))
        })?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Member {} is not assigned to space {}",
                request.member_id, request.space_id
            ))
        })?;

        if current_role == "owner" {
            let owner_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conversation_space_members WHERE space_id = ? AND role = 'owner'",
        )
        .bind(&request.space_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to validate owner count for space member removal: {}",
                e
            ))
        })?;

            if owner_count <= 1 {
                return Err(AppError::InvalidInput(
                    "Cannot remove the final owner from a space".to_string(),
                ));
            }
        }

        sqlx::query(
            r#"
        DELETE FROM conversation_space_members
        WHERE space_id = ? AND member_id = ?
        "#,
        )
        .bind(&request.space_id)
        .bind(&request.member_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to remove conversation space member: {}", e))
        })?;

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    pub async fn list_document_space_memberships(
        &self,
        document_id: String,
    ) -> Result<Vec<DocumentSpaceMembershipDto>, AppError> {
        let memberships = sqlx::query_as::<_, DocumentSpaceMembershipRow>(
            r#"
        SELECT
            cs.id AS space_id,
            cs.name AS space_name,
            cs.is_archived AS is_archived,
            dsm.created_at AS created_at
        FROM document_space_memberships dsm
        INNER JOIN conversation_spaces cs ON cs.id = dsm.space_id
        WHERE dsm.document_id = ?
        ORDER BY cs.is_archived ASC, cs.name ASC
        "#,
        )
        .bind(&document_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to list document space memberships: {}", e))
        })?;

        Ok(memberships
            .into_iter()
            .map(|row| DocumentSpaceMembershipDto {
                space_id: row.space_id,
                space_name: row.space_name,
                is_archived: row.is_archived != 0,
                created_at: row.created_at,
            })
            .collect())
    }

    pub async fn set_document_space_membership(
        &self,
        document_id: String,
        space_id: String,
        assigned: bool,
    ) -> Result<RenameConversationResponseDto, AppError> {
        let document_exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE id = ?")
                .bind(&document_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| AppError::Database(format!("Failed to verify document: {}", e)))?;

        if document_exists == 0 {
            return Err(AppError::NotFound(format!(
                "Document not found: {}",
                document_id
            )));
        }

        if assigned {
            let space_exists: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM conversation_spaces WHERE id = ?")
                    .bind(&space_id)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(|e| AppError::Database(format!("Failed to verify space: {}", e)))?;

            if space_exists == 0 {
                return Err(AppError::NotFound(format!("Space not found: {}", space_id)));
            }

            let now = Utc::now().to_rfc3339();
            sqlx::query(
                r#"
            INSERT OR IGNORE INTO document_space_memberships (document_id, space_id, created_at)
            VALUES (?, ?, ?)
            "#,
            )
            .bind(&document_id)
            .bind(&space_id)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                AppError::Database(format!("Failed to assign document to space: {}", e))
            })?;
        } else {
            sqlx::query(
                r#"
            DELETE FROM document_space_memberships
            WHERE document_id = ? AND space_id = ?
            "#,
            )
            .bind(&document_id)
            .bind(&space_id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                AppError::Database(format!("Failed to remove document space membership: {}", e))
            })?;
        }

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }

    pub async fn set_documents_space_membership(
        &self,
        document_ids: Vec<String>,
        space_id: String,
        assigned: bool,
    ) -> Result<RenameConversationResponseDto, AppError> {
        const DOCUMENT_BATCH_SIZE: usize = 250;

        let mut unique_document_ids = document_ids
            .into_iter()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect::<Vec<_>>();
        unique_document_ids.sort();
        unique_document_ids.dedup();

        if unique_document_ids.is_empty() {
            return Err(AppError::InvalidInput(
                "At least one document ID is required".to_string(),
            ));
        }

        let space_exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM conversation_spaces WHERE id = ?")
                .bind(&space_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| AppError::Database(format!("Failed to verify space: {}", e)))?;

        if space_exists == 0 {
            return Err(AppError::NotFound(format!("Space not found: {}", space_id)));
        }

        let mut existing_document_ids = HashSet::with_capacity(unique_document_ids.len());
        for document_id_batch in unique_document_ids.chunks(DOCUMENT_BATCH_SIZE) {
            let mut qb = QueryBuilder::<Sqlite>::new("SELECT id FROM documents WHERE id IN (");
            {
                let mut separated = qb.separated(", ");
                for document_id in document_id_batch {
                    separated.push_bind(document_id.as_str());
                }
            }
            qb.push(")");

            let found = qb
                .build_query_scalar::<String>()
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AppError::Database(format!("Failed to verify documents: {}", e)))?;

            for existing_id in found {
                existing_document_ids.insert(existing_id);
            }
        }

        if let Some(missing_document_id) = unique_document_ids
            .iter()
            .find(|document_id| !existing_document_ids.contains(*document_id))
        {
            return Err(AppError::NotFound(format!(
                "Document not found: {}",
                missing_document_id
            )));
        }

        if assigned {
            let now = Utc::now().to_rfc3339();
            for document_id_batch in unique_document_ids.chunks(DOCUMENT_BATCH_SIZE) {
                let mut qb = QueryBuilder::<Sqlite>::new(
                "INSERT OR IGNORE INTO document_space_memberships (document_id, space_id, created_at) ",
            );
                qb.push_values(document_id_batch.iter(), |mut builder, document_id| {
                    builder
                        .push_bind(document_id.as_str())
                        .push_bind(space_id.as_str())
                        .push_bind(now.as_str());
                });

                qb.build().execute(&self.pool).await.map_err(|e| {
                    AppError::Database(format!("Failed to assign documents to space: {}", e))
                })?;
            }
        } else {
            for document_id_batch in unique_document_ids.chunks(DOCUMENT_BATCH_SIZE) {
                let mut qb = QueryBuilder::<Sqlite>::new(
                    "DELETE FROM document_space_memberships WHERE space_id = ",
                );
                qb.push_bind(space_id.as_str())
                    .push(" AND document_id IN (");
                {
                    let mut separated = qb.separated(", ");
                    for document_id in document_id_batch {
                        separated.push_bind(document_id.as_str());
                    }
                }
                qb.push(")");

                qb.build().execute(&self.pool).await.map_err(|e| {
                    AppError::Database(format!("Failed to remove documents from space: {}", e))
                })?;
            }
        }

        Ok(RenameConversationResponseDto {
            status: "success".to_string(),
        })
    }
}
