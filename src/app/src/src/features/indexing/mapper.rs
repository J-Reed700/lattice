//! # Indexing Mapper
//!
//! Converts between domain indexing models and DTOs.
//!
//! This mapper handles conversion between:
//! - `ChunkingStrategy` (domain) ↔ `ChunkingStrategyDto` (DTO)

use crate::features::indexing::dto::ChunkingStrategyDto;
use crate::domain::value_objects::chunking_strategy::ChunkingStrategy;

/// Mapper for indexing-related conversions.
pub struct IndexingMapper;

impl IndexingMapper {
    /// Convert ChunkingStrategyDto to domain.
    ///
    /// # Arguments
    ///
    /// * `dto` - DTO chunking strategy
    ///
    /// # Returns
    ///
    /// Domain ChunkingStrategy value object
    pub fn chunking_strategy_to_domain(dto: ChunkingStrategyDto) -> ChunkingStrategy {
        match dto {
            ChunkingStrategyDto::FixedSize { size } => ChunkingStrategy::FixedSize { size },
            ChunkingStrategyDto::Semantic { max_tokens } => {
                ChunkingStrategy::Semantic { max_tokens }
            }
            ChunkingStrategyDto::Paragraph => {
                // Map Paragraph to Semantic with reasonable token limit
                ChunkingStrategy::Semantic { max_tokens: 512 }
            }
            ChunkingStrategyDto::Sentence => {
                // Map Sentence to Semantic with smaller token limit
                ChunkingStrategy::Semantic { max_tokens: 256 }
            }
        }
    }

    /// Convert domain ChunkingStrategy to DTO.
    ///
    /// # Arguments
    ///
    /// * `strategy` - Domain chunking strategy
    ///
    /// # Returns
    ///
    /// DTO representation of chunking strategy
    pub fn chunking_strategy_to_dto(strategy: ChunkingStrategy) -> ChunkingStrategyDto {
        match strategy {
            ChunkingStrategy::FixedSize { size } => ChunkingStrategyDto::FixedSize { size },
            ChunkingStrategy::Semantic { max_tokens } => {
                ChunkingStrategyDto::Semantic { max_tokens }
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_size_dto_to_domain() {
        let dto = ChunkingStrategyDto::FixedSize { size: 1024 };
        let domain = IndexingMapper::chunking_strategy_to_domain(dto);

        assert!(matches!(domain, ChunkingStrategy::FixedSize { size: 1024 }));
    }

    #[test]
    fn test_semantic_dto_to_domain() {
        let dto = ChunkingStrategyDto::Semantic { max_tokens: 512 };
        let domain = IndexingMapper::chunking_strategy_to_domain(dto);

        assert!(matches!(
            domain,
            ChunkingStrategy::Semantic { max_tokens: 512 }
        ));
    }

    #[test]
    fn test_paragraph_dto_to_domain() {
        let dto = ChunkingStrategyDto::Paragraph;
        let domain = IndexingMapper::chunking_strategy_to_domain(dto);

        // Paragraph maps to Semantic with 512 tokens
        assert!(matches!(
            domain,
            ChunkingStrategy::Semantic { max_tokens: 512 }
        ));
    }

    #[test]
    fn test_sentence_dto_to_domain() {
        let dto = ChunkingStrategyDto::Sentence;
        let domain = IndexingMapper::chunking_strategy_to_domain(dto);

        // Sentence maps to Semantic with 256 tokens
        assert!(matches!(
            domain,
            ChunkingStrategy::Semantic { max_tokens: 256 }
        ));
    }

    #[test]
    fn test_domain_to_dto_fixed_size() {
        let domain = ChunkingStrategy::FixedSize { size: 2048 };
        let dto = IndexingMapper::chunking_strategy_to_dto(domain);

        assert!(matches!(dto, ChunkingStrategyDto::FixedSize { size: 2048 }));
    }

    #[test]
    fn test_domain_to_dto_semantic() {
        let domain = ChunkingStrategy::Semantic { max_tokens: 1024 };
        let dto = IndexingMapper::chunking_strategy_to_dto(domain);

        assert!(matches!(
            dto,
            ChunkingStrategyDto::Semantic { max_tokens: 1024 }
        ));
    }
}
