//! Compatibility scoring and ranked model recommendations.
use super::*;

/// Detailed compatibility analysis between a model and system.
///
/// This value object represents the output of the compatibility scoring
/// algorithm, providing:
/// - Overall compatibility level
/// - Detailed factor scores (RAM, GPU, disk)
/// - Performance estimates
/// - Recommendations
///
/// # Scoring Algorithm
///
/// 1. **Critical Checks** (instant fail):
///    - RAM: available >= minimum (else Incompatible)
///    - Disk: available >= size (else Incompatible)
///
/// 2. **Factor Scores** (0-100):
///    - RAM Score: % of recommended RAM available
///    - GPU Score: 100 if GPU available, 50 otherwise
///    - Disk Score: 100 if ample space, 75 if tight
///
/// 3. **Overall Score**:
///    - RAM: 50% weight
///    - GPU: 30% weight
///    - Disk: 20% weight
///
/// 4. **Compatibility Level**:
///    - > 85: Excellent
///    - 60-85: Good
///    - < 60: Poor
///    - Fails critical: Incompatible
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityScore {
    /// Overall compatibility level
    pub compatibility_level: CompatibilityLevel,
    /// Overall compatibility score (0-100)
    pub overall_score: f64,
    /// RAM compatibility score (0-100)
    pub ram_score: f64,
    /// GPU compatibility score (0-100)
    pub gpu_score: f64,
    /// Disk space compatibility score (0-100)
    pub disk_score: f64,
    /// Estimated tokens per second (for LLMs)
    pub estimated_tokens_per_second: Option<f64>,
    /// Estimated model loading time in seconds
    pub estimated_loading_time_seconds: f64,
    /// Human-readable recommendations
    pub recommendations: Vec<String>,
    /// Blockers preventing model from running
    pub blockers: Vec<String>,
}

impl CompatibilityScore {
    /// Check if model is compatible (can run).
    pub fn is_compatible(&self) -> bool {
        !matches!(self.compatibility_level, CompatibilityLevel::Incompatible)
    }

    /// Check if model is recommended (Good or Excellent).
    pub fn is_recommended(&self) -> bool {
        matches!(
            self.compatibility_level,
            CompatibilityLevel::Good | CompatibilityLevel::Excellent
        )
    }
}

/// Model recommendation with compatibility analysis.
///
/// This aggregate combines:
/// - Model metadata
/// - Compatibility score
/// - System-specific performance estimates
///
/// Used for presenting ranked model recommendations to users.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRecommendation {
    /// Model metadata
    pub model: ModelMetadata,
    /// Compatibility analysis
    pub compatibility: CompatibilityScore,
    /// Ranking score for sorting (0-100)
    pub ranking_score: f64,
}

impl ModelRecommendation {
    /// Create a new recommendation.
    pub fn new(model: ModelMetadata, compatibility: CompatibilityScore) -> Self {
        // Ranking score = compatibility score, adjusted for performance tier
        let tier_bonus = match model.performance_tier {
            PerformanceTier::Fast => 5.0,
            PerformanceTier::Balanced => 0.0,
            PerformanceTier::Accurate => -5.0,
        };

        let ranking_score = (compatibility.overall_score + tier_bonus).clamp(0.0, 100.0);

        Self {
            model,
            compatibility,
            ranking_score,
        }
    }

    /// Sort recommendations by ranking score (descending).
    pub fn sort_by_ranking(recommendations: &mut [ModelRecommendation]) {
        recommendations.sort_by(|a, b| {
            b.ranking_score
                .partial_cmp(&a.ranking_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

/// Domain service for scoring model compatibility with system capabilities.
///
/// This service implements the compatibility scoring algorithm as pure business
/// logic with no infrastructure dependencies.
///
/// # Algorithm
///
/// See [`CompatibilityScore`] documentation for detailed algorithm description.
pub struct CompatibilityScorer;

impl CompatibilityScorer {
    /// Create a new compatibility scorer.
    pub fn new() -> Self {
        Self
    }

    /// Score compatibility between a model and system capabilities.
    pub fn score_compatibility(
        &self,
        model: &ModelMetadata,
        capabilities: &SystemCapabilities,
    ) -> Result<CompatibilityScore> {
        let mut blockers = Vec::new();
        let mut recommendations = Vec::new();

        if !capabilities.has_sufficient_ram(model.minimum_ram_gb) {
            blockers.push(format!(
                "Insufficient RAM: need {} GB, have {} GB total",
                model.minimum_ram_gb, capabilities.total_ram_gb
            ));
        }

        if !capabilities.has_sufficient_disk(model.size_gb) {
            blockers.push(format!(
                "Insufficient disk space: need {} GB, have {} GB available",
                model.size_gb, capabilities.available_disk_gb
            ));
        }

        // If critical checks fail, return Incompatible
        if !blockers.is_empty() {
            return Ok(CompatibilityScore {
                compatibility_level: CompatibilityLevel::Incompatible,
                overall_score: 0.0,
                ram_score: 0.0,
                gpu_score: 0.0,
                disk_score: 0.0,
                estimated_tokens_per_second: None,
                estimated_loading_time_seconds: 0.0,
                recommendations: vec![],
                blockers,
            });
        }

        // RAM Score: percentage of recommended RAM available
        let ram_ratio = capabilities.total_ram_gb / model.recommended_ram_gb;
        let ram_score = (ram_ratio * 100.0).clamp(0.0, 100.0);

        if ram_score < 100.0 {
            recommendations.push(format!(
                "Model will run, but {} GB RAM recommended for best performance",
                model.recommended_ram_gb
            ));
        }

        // GPU Score: 100 if GPU available, 50 if CPU-only
        let gpu_score = if capabilities.has_gpu_acceleration() {
            100.0
        } else {
            50.0
        };

        if !capabilities.has_gpu_acceleration() {
            recommendations
                .push("Running on CPU only. GPU would improve performance significantly.".into());
        }

        // Disk Score: 100 if ample space (10x model size), 75 if tight
        let disk_ratio = capabilities.available_disk_gb / model.size_gb;
        let disk_score = if disk_ratio >= 10.0 {
            100.0
        } else if disk_ratio >= 2.0 {
            75.0
        } else {
            50.0
        };

        if disk_score < 100.0 {
            recommendations.push(format!(
                "Low disk space. Recommend {} GB free for optimal performance",
                model.size_gb * 5.0
            ));
        }

        let overall_score = (ram_score * 0.5) + (gpu_score * 0.3) + (disk_score * 0.2);

        let compatibility_level = if overall_score >= 85.0 {
            CompatibilityLevel::Excellent
        } else if overall_score >= 60.0 {
            CompatibilityLevel::Good
        } else {
            CompatibilityLevel::Poor
        };

        // Estimate tokens/second for LLMs
        let estimated_tokens_per_second = if matches!(model.category, ModelCategory::LLM) {
            let base_tokens_per_sec = match model.performance_tier {
                PerformanceTier::Fast => 30.0,
                PerformanceTier::Balanced => 20.0,
                PerformanceTier::Accurate => 10.0,
            };

            // Adjust for GPU
            let gpu_multiplier = if capabilities.has_gpu_acceleration() {
                match capabilities.gpu_type {
                    GpuType::AppleSilicon => 2.5,
                    GpuType::Nvidia => 3.0,
                    GpuType::AMD => 2.0,
                    _ => 1.0,
                }
            } else {
                1.0
            };

            // Adjust for RAM
            let ram_multiplier = if capabilities.total_ram_gb >= model.recommended_ram_gb {
                1.0
            } else {
                0.7
            };

            Some(base_tokens_per_sec * gpu_multiplier * ram_multiplier)
        } else {
            None
        };

        // Estimate loading time (based on model size and disk speed)
        let estimated_loading_time_seconds = model.size_gb * 2.0; // ~2 seconds per GB

        Ok(CompatibilityScore {
            compatibility_level,
            overall_score,
            ram_score,
            gpu_score,
            disk_score,
            estimated_tokens_per_second,
            estimated_loading_time_seconds,
            recommendations,
            blockers,
        })
    }
}

impl Default for CompatibilityScorer {
    fn default() -> Self {
        Self::new()
    }
}
