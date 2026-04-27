use crate::features::mentions::dto::{MentionDto, MentionWithContextDto};
use crate::application::ports::{MentionData, MentionWithContextData};

pub struct MentionMapper;

impl MentionMapper {
    pub fn new() -> Self {
        Self
    }

    pub fn mention_data_to_dto(&self, data: MentionData) -> MentionDto {
        MentionDto {
            id: data.id,
            name: data.name,
            mention_type: data.mention_type,
            metadata: data.metadata,
            created_at: data.created_at,
        }
    }

    pub fn mention_with_context_data_to_dto(
        &self,
        data: MentionWithContextData,
    ) -> MentionWithContextDto {
        MentionWithContextDto {
            id: data.mention.id,
            name: data.mention.name,
            mention_type: data.mention.mention_type,
            document_id: data.document_id,
            context: data.context.unwrap_or_default(),
            position: data.position.unwrap_or(0),
            created_at: data.mention.created_at,
        }
    }
}

impl Default for MentionMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mention_data_to_dto() {
        let mapper = MentionMapper::new();
        let data = MentionData {
            id: "mention-1".to_string(),
            name: "John Doe".to_string(),
            mention_type: "person".to_string(),
            metadata: Some("metadata".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let dto = mapper.mention_data_to_dto(data);

        assert_eq!(dto.id, "mention-1");
        assert_eq!(dto.name, "John Doe");
        assert_eq!(dto.mention_type, "person");
        assert_eq!(dto.metadata, Some("metadata".to_string()));
        assert_eq!(dto.created_at, "2024-01-01T00:00:00Z");
    }

    #[test]
    fn test_mention_with_context_data_to_dto() {
        let mapper = MentionMapper::new();
        let data = MentionWithContextData {
            mention: MentionData {
                id: "mention-1".to_string(),
                name: "John Doe".to_string(),
                mention_type: "person".to_string(),
                metadata: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
            },
            document_id: "doc-123".to_string(),
            context: Some("met @[John Doe]".to_string()),
            position: Some(4),
        };

        let dto = mapper.mention_with_context_data_to_dto(data);

        assert_eq!(dto.id, "mention-1");
        assert_eq!(dto.name, "John Doe");
        assert_eq!(dto.context, "met @[John Doe]");
        assert_eq!(dto.position, 4);
    }
}
