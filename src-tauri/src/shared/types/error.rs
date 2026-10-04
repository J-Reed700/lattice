/// Error type for domain type validation
#[derive(Debug, thiserror::Error)]
pub enum DomainTypeError {
    #[error("Invalid ID format: {0}")]
    InvalidId(String),

    #[error("Empty value not allowed")]
    EmptyValue,

    #[error("Value too long: {0} (max: {1})")]
    ValueTooLong(usize, usize),
}
