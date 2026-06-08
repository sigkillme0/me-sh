use crate::prelude::*;

#[derive(Debug, Error, miette::Diagnostic)]
pub(crate) enum MeshError {
    #[error("{0}")]
    Message(String),
}

pub(crate) fn err<T>(message: impl Into<String>) -> Result<T> {
    Err(MeshError::Message(message.into()).into())
}
