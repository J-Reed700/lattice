//! Scope, explicit user memory edits and time metadata on the same ledger.
use super::ConversationRepository;
use crate::domain::conversation_memory::compute_digest;
use crate::features::conversation::knowledge_dto::{
    KnowledgeItemDto, KnowledgeRequestDto, KnowledgeResponseDto,
};
use crate::features::conversation::memory_dto::MemoryEvidenceDto;
use crate::shared::error::{AppError, Result};

#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    conversation_id: String,
    conversation_title: String,
    label: String,
    kind: String,
    state: String,
    scope: String,
    learned_at: String,
    valid_from: Option<String>,
    valid_until: Option<String>,
    verified_at: Option<String>,
    forgotten: bool,
    superseded_by: Option<String>,
}

impl ConversationRepository {
    pub async fn knowledge(&self, request: &KnowledgeRequestDto) -> Result<KnowledgeResponseDto> {
        if request.action != "list" {
            self.change_knowledge(request).await?;
        }
        self.list_knowledge(&request.conversation_id, request.offset.unwrap_or(0), false)
            .await
    }

    pub async fn list_knowledge(
        &self,
        conversation: &str,
        offset: i64,
        for_prompt: bool,
    ) -> Result<KnowledgeResponseDto> {
        let mut tx = self.pool.begin().await?;
        let space: String = sqlx::query_scalar("SELECT space_id FROM conversations WHERE id=?")
            .bind(conversation)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::NotFound("Conversation not found".into()))?;
        let limit = if for_prompt { 1001 } else { 51 };
        let rows = sqlx::query_as::<_, Row>("SELECT i.id,i.conversation_id,c.title AS conversation_title,i.label,i.kind,i.state,COALESCE(a.scope,'conversation') AS scope,i.created_at AS learned_at,a.valid_from,a.valid_until,a.verified_at,COALESCE(a.forgotten,0) AS forgotten,i.superseded_by FROM conversation_memory_items i JOIN conversations c ON c.id=i.conversation_id LEFT JOIN conversation_memory_attributes a ON a.item_id=i.id LEFT JOIN conversation_memory_state s ON s.conversation_id=i.conversation_id WHERE (i.conversation_id=? OR a.scope='personal' OR (a.scope='space' AND a.space_id=?)) AND (?=0 OR (i.state='active' AND COALESCE(a.forgotten,0)=0 AND s.validity='ready' AND s.schema_version=1 AND (a.valid_from IS NULL OR julianday(a.valid_from)<=julianday('now')) AND (a.valid_until IS NULL OR julianday(a.valid_until)>julianday('now')))) ORDER BY i.created_at DESC,i.id LIMIT ? OFFSET ?")
            .bind(conversation).bind(&space).bind(for_prompt).bind(limit).bind(offset.max(0)).fetch_all(&mut *tx).await?;
        let page_size = if for_prompt { 1000 } else { 50 };
        let has_more = rows.len() > page_size;
        let mut items = Vec::new();
        for row in rows.into_iter().take(page_size) {
            #[derive(sqlx::FromRow)]
            struct Evidence {
                message_id: String,
                sequence: i64,
                role: String,
                purpose: String,
                start_byte: i64,
                end_byte: i64,
                content_digest: String,
                content: String,
            }
            let evidence=sqlx::query_as::<_,Evidence>("SELECT e.message_id,e.sequence,e.role,e.purpose,e.start_byte,e.end_byte,e.content_digest,m.content FROM conversation_memory_evidence e JOIN conversation_messages m ON m.id=e.message_id AND m.conversation_id=? WHERE e.item_id=? ORDER BY e.ordinal")
                .bind(&row.conversation_id).bind(&row.id).fetch_all(&mut *tx).await?;
            let evidence: Vec<_> = evidence
                .into_iter()
                .map(|e| {
                    let text = if compute_digest(&e.content) == e.content_digest {
                        e.content
                            .get(e.start_byte as usize..e.end_byte as usize)
                            .map(str::to_owned)
                    } else {
                        None
                    };
                    MemoryEvidenceDto {
                        message_id: e.message_id,
                        sequence: e.sequence,
                        role: e.role,
                        purpose: e.purpose,
                        start_byte: e.start_byte as u32,
                        end_byte: e.end_byte as u32,
                        text,
                    }
                })
                .collect();
            if for_prompt && (evidence.is_empty() || evidence.iter().any(|e| e.text.is_none())) {
                continue;
            }
            let reason = if row.conversation_id == conversation {
                "Saved in this conversation"
            } else if row.scope == "personal" {
                "Explicitly shared with all conversations"
            } else {
                "Shared with this space"
            };
            items.push(KnowledgeItemDto {
                id: row.id,
                conversation_id: row.conversation_id,
                conversation_title: row.conversation_title,
                label: row.label,
                kind: row.kind,
                state: row.state,
                scope: row.scope,
                learned_at: row.learned_at,
                valid_from: row.valid_from,
                valid_until: row.valid_until,
                verified_at: row.verified_at,
                forgotten: row.forgotten,
                superseded_by: row.superseded_by,
                evidence,
                availability_reason: reason.into(),
            });
        }
        let raw: Option<String> = sqlx::query_scalar("SELECT json_extract(metadata,'$.memory.items') FROM conversation_messages WHERE conversation_id=? AND role='assistant' AND status='completed' ORDER BY sequence DESC LIMIT 1")
            .bind(conversation).fetch_optional(&mut *tx).await?.flatten();
        let last_answer_memory_ids = raw
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        tx.commit().await?;
        Ok(KnowledgeResponseDto {
            items,
            has_more,
            last_answer_memory_ids,
        })
    }

    async fn change_knowledge(&self, r: &KnowledgeRequestDto) -> Result<()> {
        let scope = r.scope.as_deref().unwrap_or("conversation");
        if !["conversation", "space", "personal"].contains(&scope) {
            return Err(AppError::InvalidInput("Unknown memory scope".into()));
        }
        let valid_from = timestamp(r.valid_from.as_deref())?;
        let valid_until = timestamp(r.valid_until.as_deref())?;
        if let (Some(a), Some(b)) = (&valid_from, &valid_until) {
            if a >= b {
                return Err(AppError::InvalidInput(
                    "Valid until must follow valid from".into(),
                ));
            }
        }
        let mut tx = self.pool.begin().await?;
        let space: String = sqlx::query_scalar("SELECT space_id FROM conversations WHERE id=?")
            .bind(&r.conversation_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::NotFound("Conversation not found".into()))?;
        let target = if r.action == "remember" {
            None
        } else {
            let id = r
                .item_id
                .as_deref()
                .ok_or_else(|| AppError::InvalidInput("Memory item is required".into()))?;
            // A shared read never grants mutation rights through a foreign thread.
            let owner:Option<String>=sqlx::query_scalar("SELECT id FROM conversation_memory_items WHERE id=? AND conversation_id=? AND state='active'").bind(id).bind(&r.conversation_id).fetch_optional(&mut *tx).await?;
            Some(owner.ok_or_else(|| {
                AppError::NotFound("Open the source conversation to change this memory".into())
            })?)
        };
        let now = chrono::Utc::now().to_rfc3339();
        let mut changed = target.clone();
        if r.action == "remember" || r.action == "correct" {
            let text = r
                .text
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty() && s.len() <= 8000)
                .ok_or_else(|| {
                    AppError::InvalidInput("Memory must contain 1–8000 bytes of text".into())
                })?;
            let sequence = Self::allocate_sequence(&mut tx, &r.conversation_id).await?;
            let message = uuid::Uuid::new_v4().to_string();
            let id = uuid::Uuid::new_v4().to_string();
            changed = Some(id.clone());
            let digest = compute_digest(text);
            let tokens = (text.chars().count() / 3 + 1) as i64;
            sqlx::query("INSERT INTO conversation_messages(id,conversation_id,role,content,tokens,status,sequence,content_digest,metadata) VALUES (?,?,'user',?,?,'completed',?,?, '{\"memoryNote\":true}')")
                .bind(&message).bind(&r.conversation_id).bind(text).bind(tokens).bind(sequence).bind(&digest).execute(&mut *tx).await?;
            sqlx::query("UPDATE conversations SET message_count=message_count+1,total_tokens=total_tokens+?,updated_at=? WHERE id=?").bind(tokens).bind(&now).bind(&r.conversation_id).execute(&mut *tx).await?;
            let kind: String = if let Some(target) = &target {
                sqlx::query_scalar("SELECT kind FROM conversation_memory_items WHERE id=?")
                    .bind(target)
                    .fetch_one(&mut *tx)
                    .await?
            } else {
                let kind = r.kind.as_deref().unwrap_or("user_fact");
                if ![
                    "user_fact",
                    "preference",
                    "constraint",
                    "goal",
                    "decision",
                    "open_question",
                ]
                .contains(&kind)
                {
                    return Err(AppError::InvalidInput("Choose a user memory kind".into()));
                }
                kind.into()
            };
            sqlx::query("INSERT INTO conversation_memory_items(id,conversation_id,kind,label,created_at_sequence,changed_at_sequence,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?)")
                .bind(&id).bind(&r.conversation_id).bind(kind).bind(text).bind(sequence).bind(sequence).bind(&now).bind(&now).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO conversation_memory_evidence(item_id,ordinal,message_id,sequence,role,start_byte,end_byte,content_digest,purpose) VALUES (?,0,?,?,'user',0,?,?,'assertion')")
                .bind(&id).bind(&message).bind(sequence).bind(text.len() as i64).bind(digest).execute(&mut *tx).await?;
            if let Some(target) = &target {
                sqlx::query("UPDATE conversation_memory_items SET state='superseded',superseded_by=?,changed_at_sequence=?,revision=revision+1,updated_at=? WHERE id=? AND state='active'").bind(&id).bind(sequence).bind(&now).bind(target).execute(&mut *tx).await?;
            }
            sqlx::query("INSERT INTO conversation_memory_attributes(item_id,scope,space_id,valid_from,valid_until,verified_at) VALUES (?,?,?,?,?,?) ON CONFLICT(item_id) DO UPDATE SET scope=CASE WHEN ? THEN excluded.scope ELSE scope END,space_id=CASE WHEN ? THEN excluded.space_id ELSE space_id END,valid_from=excluded.valid_from,valid_until=excluded.valid_until,verified_at=excluded.verified_at")
                .bind(&id).bind(scope).bind(if scope=="space" {Some(&space)} else {None}).bind(valid_from).bind(valid_until).bind(None::<&str>).bind(r.scope.is_some()).bind(r.scope.is_some()).execute(&mut *tx).await?;
        } else {
            let id = target
                .as_deref()
                .ok_or_else(|| AppError::InvalidInput("Missing memory".into()))?;
            sqlx::query("INSERT OR IGNORE INTO conversation_memory_attributes(item_id) VALUES (?)")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            match r.action.as_str() {
                "scope" => {
                    sqlx::query("UPDATE conversation_memory_attributes SET scope=?,space_id=? WHERE item_id=?").bind(scope).bind(if scope=="space" {Some(&space)} else {None}).bind(id).execute(&mut *tx).await?;
                }
                "dates" => {
                    sqlx::query("UPDATE conversation_memory_attributes SET valid_from=?,valid_until=? WHERE item_id=?").bind(valid_from).bind(valid_until).bind(id).execute(&mut *tx).await?;
                }
                "verify" => {
                    sqlx::query(
                        "UPDATE conversation_memory_attributes SET verified_at=? WHERE item_id=?",
                    )
                    .bind(&now)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                }
                "forget" => {
                    sqlx::query("UPDATE conversation_memory_items SET state='resolved',revision=revision+1,updated_at=? WHERE id=?")
                        .bind(&now).bind(id).execute(&mut *tx).await?;
                    sqlx::query("INSERT OR IGNORE INTO conversation_memory_suppressions(message_id,start_byte,end_byte,content_digest) SELECT message_id,start_byte,end_byte,content_digest FROM conversation_memory_evidence WHERE item_id=? AND purpose='assertion'")
                        .bind(id).execute(&mut *tx).await?;
                    sqlx::query("UPDATE conversation_memory_attributes SET forgotten=1,scope='conversation',space_id=NULL WHERE item_id=?").bind(id).execute(&mut *tx).await?;
                }
                _ => return Err(AppError::InvalidInput("Unknown memory action".into())),
            }
        }
        // Invalidate in-flight model commits. A user's edit cannot be overwritten
        // by a patch extracted before the edit. The original watermark stays put.
        sqlx::query("INSERT INTO conversation_memory_state(conversation_id,memory_revision) VALUES (?,1) ON CONFLICT(conversation_id) DO UPDATE SET memory_revision=memory_revision+1,updated_at=CURRENT_TIMESTAMP")
            .bind(&r.conversation_id).execute(&mut *tx).await?;
        // An old prose summary can contain a corrected/forgotten value. It is
        // regenerated by consolidation, never served as current after an edit.
        if ["correct", "forget"].contains(&r.action.as_str()) {
            sqlx::query("DELETE FROM conversation_summaries WHERE conversation_id=?")
                .bind(&r.conversation_id)
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query("UPDATE conversation_summaries SET memory_revision=(SELECT memory_revision FROM conversation_memory_state WHERE conversation_id=?) WHERE conversation_id=?").bind(&r.conversation_id).bind(&r.conversation_id).execute(&mut *tx).await?;
        }
        let affected: Vec<&str> = target
            .iter()
            .chain(changed.iter())
            .map(String::as_str)
            .collect();
        sqlx::query("INSERT INTO conversation_memory_events(id,conversation_id,operation_id,memory_revision,operation,item_ids) SELECT ?,?,?,memory_revision,?,? FROM conversation_memory_state WHERE conversation_id=?")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&r.conversation_id).bind(uuid::Uuid::new_v4().to_string()).bind(format!("user_{}",r.action)).bind(serde_json::to_string(&affected)?).bind(&r.conversation_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
fn timestamp(value: Option<&str>) -> Result<Option<String>> {
    value
        .filter(|s| !s.is_empty())
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|d| {
                    d.with_timezone(&chrono::Utc)
                        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                })
                .map_err(|_| AppError::InvalidInput("Dates must be ISO 8601 timestamps".into()))
        })
        .transpose()
}

impl ConversationRepository {
    pub(super) async fn prepare_knowledge(
        &self,
        conversation: &str,
        query: &str,
    ) -> Result<crate::application::ports::conversation_memory::PreparedMemory> {
        let revisions = self.knowledge_revisions(conversation).await?;
        let response = self.list_knowledge(conversation, 0, true).await?;
        if response.has_more {
            return Err(AppError::InvalidState(
                "More than 1000 applicable memories; narrow their sharing scope before continuing"
                    .into(),
            ));
        }
        let terms: Vec<_> = query
            .split_whitespace()
            .filter(|s| s.len() > 2)
            .take(32)
            .map(str::to_lowercase)
            .collect();
        let semantic = self.knowledge_semantic_scores(conversation, query).await;
        let mut prepared =
            crate::application::ports::conversation_memory::PreparedMemory::default();
        let mut optional = Vec::new();
        for item in response.items {
            let text=serde_json::json!({"memory_id":item.id,"source_conversation":item.conversation_id,"source_title":item.conversation_title,"scope":item.scope,"kind":item.kind,"generated_label":item.label,"learned_at":item.learned_at,"valid_from":item.valid_from,"valid_until":item.valid_until,"last_user_verified_at":item.verified_at,"evidence":item.evidence,"why_available":item.availability_reason}).to_string();
            if ["constraint", "goal", "decision", "unresolved_change"].contains(&item.kind.as_str())
            {
                prepared.mandatory.push(text);
            } else {
                let haystack = format!(
                    "{} {}",
                    item.label,
                    item.evidence
                        .iter()
                        .filter_map(|e| e.text.as_deref())
                        .collect::<Vec<_>>()
                        .join(" ")
                )
                .to_lowercase();
                let matches = terms
                    .iter()
                    .filter(|term| haystack.contains(term.as_str()))
                    .count();
                optional.push((
                    matches as f32 + semantic.get(&item.id).copied().unwrap_or(0.0),
                    text,
                ));
            }
        }
        optional.sort_by(|a, b| b.0.total_cmp(&a.0));
        prepared.optional = optional
            .into_iter()
            .take(24)
            .map(|(_, text)| text)
            .collect();
        if self.knowledge_revisions(conversation).await? != revisions {
            return Err(AppError::ConcurrentModification {
                resource: "conversation memory".into(),
                details: "Memory changed while resolving evidence; retry".into(),
            });
        }
        prepared.revisions = Some(revisions);
        Ok(prepared)
    }
}

impl ConversationRepository {
    pub(super) async fn search_knowledge(
        &self,
        conversation: &str,
        query: &str,
        history: bool,
        max_bytes: usize,
    ) -> Result<serde_json::Value> {
        let terms: Vec<_> = query
            .split_whitespace()
            .take(32)
            .map(str::to_lowercase)
            .collect();
        let mut ranked = Vec::new();
        let mut more = false;
        for page in 0..20 {
            let response = self.list_knowledge(conversation, page * 50, false).await?;
            more = response.has_more;
            for item in response.items {
                if item.forgotten
                    || item.evidence.is_empty()
                    || item.evidence.iter().any(|e| e.text.is_none())
                {
                    continue;
                }
                let state = self.load_memory_state(&item.conversation_id).await?;
                if !state.is_schema_supported()
                    || state.validity != crate::domain::conversation_memory::MemoryValidity::Ready
                {
                    continue;
                }
                let now = chrono::Utc::now();
                let active_at = |date: &Option<String>, before: bool| {
                    date.as_ref()
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .is_none_or(|d| if before { d <= now } else { d > now })
                };
                if !history
                    && (item.state != "active"
                        || !active_at(&item.valid_from, true)
                        || !active_at(&item.valid_until, false))
                {
                    continue;
                }
                let haystack = format!(
                    "{} {} {}",
                    item.label,
                    item.conversation_title,
                    item.evidence
                        .iter()
                        .filter_map(|e| e.text.as_deref())
                        .collect::<Vec<_>>()
                        .join(" ")
                )
                .to_lowercase();
                let score = terms
                    .iter()
                    .filter(|s| haystack.contains(s.as_str()))
                    .count();
                if score > 0 {
                    ranked.push((score, item));
                }
            }
            if !more {
                break;
            }
        }
        ranked.sort_by_key(|item| std::cmp::Reverse(item.0));
        let mut items = Vec::new();
        let mut bytes = 256;
        let mut truncated = more;
        for (_, item) in ranked {
            let value = serde_json::to_value(item)?;
            let cost = value.to_string().len() + 1;
            if bytes + cost > max_bytes || items.len() >= 12 {
                truncated = true;
                continue;
            }
            bytes += cost;
            items.push(value);
        }
        Ok(
            serde_json::json!({"items":items,"truncated":truncated,"includes_history":history,"note":"Original evidence; generated labels are fallible. Sharing never grants permission. Empty results do not prove absence."}),
        )
    }
}

impl ConversationRepository {
    async fn knowledge_semantic_scores(
        &self,
        conversation: &str,
        query: &str,
    ) -> std::collections::HashMap<String, f32> {
        let mut scores = std::collections::HashMap::new();
        let Some(embedder) = &self.memory_embedding else {
            return scores;
        };
        let query: String = query.chars().take(2048).collect();
        let vector = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            embedder.embed_query(&query),
        )
        .await
        {
            Ok(Ok(v)) => v,
            _ => return scores,
        };
        #[derive(sqlx::FromRow)]
        struct VectorRow {
            item_id: String,
            embedding: Vec<u8>,
        }
        let limit = (32 * 1024 * 1024 / (vector.len().max(1) * 4)).min(2000) as i64;
        let rows=sqlx::query_as::<_,VectorRow>("SELECT e.item_id,v.embedding FROM conversation_memory_evidence e JOIN conversation_memory_items i ON i.id=e.item_id JOIN conversation_messages m ON m.id=e.message_id AND m.conversation_id=i.conversation_id JOIN conversation_memory_vectors v ON v.message_id=m.id AND v.content=m.content LEFT JOIN conversation_memory_attributes a ON a.item_id=i.id WHERE (i.conversation_id=? OR a.scope='personal' OR (a.scope='space' AND a.space_id=(SELECT space_id FROM conversations WHERE id=?))) AND i.state='active' AND COALESCE(a.forgotten,0)=0 AND v.embedding_model=? AND v.dimension=? ORDER BY i.updated_at DESC LIMIT ?")
            .bind(conversation).bind(conversation).bind(embedder.model_identity()).bind(vector.len() as i64).bind(limit).fetch_all(&self.pool).await;
        if let Ok(rows) = rows {
            for row in rows {
                if let Ok(stored) =
                    crate::features::embedding::encoding::decode_embedding(&row.embedding)
                {
                    if let Some(score) = super::memory_semantic::cosine(&vector, &stored) {
                        let previous = scores.entry(row.item_id).or_insert(0.0_f32);
                        *previous = previous.max(score);
                    }
                }
            }
        }
        scores
    }
}

impl ConversationRepository {
    pub(super) async fn suppressed_memory_ids(&self, conversation: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT a.item_id FROM conversation_memory_attributes a JOIN conversation_memory_items i ON i.id=a.item_id WHERE i.conversation_id=? AND (a.forgotten=1 OR julianday(a.valid_from)>julianday('now') OR julianday(a.valid_until)<=julianday('now'))")
            .bind(conversation).fetch_all(&self.pool).await?)
    }
}

impl ConversationRepository {
    async fn knowledge_revisions(&self, conversation: &str) -> Result<(i64, i64)> {
        Ok(sqlx::query_as("SELECT COALESCE(s.memory_revision,0),c.transcript_revision FROM conversations c LEFT JOIN conversation_memory_state s ON s.conversation_id=c.id WHERE c.id=?")
            .bind(conversation).fetch_one(&self.pool).await?)
    }
}
