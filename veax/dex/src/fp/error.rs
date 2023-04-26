/// Math-specific error type
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Encountered NaN")]
    NaN,
    #[error("Attempted convert negative value to unsigned")]
    NegativeToUnsigned,
    #[error("Numeric overflow")]
    Overflow,
    #[error("Precision loss")]
    PrecisionLoss,
}
