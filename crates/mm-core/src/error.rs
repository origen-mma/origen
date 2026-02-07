use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("JSON parsing error: {0}")]
    JsonError(String),

    #[error("XML parsing error: {0}")]
    XmlError(String),

    #[error("Avro parsing error: {0}")]
    AvroError(String),

    #[error("Unknown topic: {0}")]
    UnknownTopic(String),

    #[error("Missing required field: {0}")]
    MissingField(&'static str),

    #[error("Invalid format: {0}")]
    InvalidFormat(String),
}
