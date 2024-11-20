use miette::Diagnostic;
use streamstore::{
    client::{ClientError, ParseError},
    types::ConvertError,
};
use thiserror::Error;

use crate::{basin::BasinServiceError, config::S2ConfigError, stream::StreamServiceError};

const HELP: &str = color_print::cstr!(
    "\n<cyan><bold>Notice something wrong?</bold></cyan>\n\n\
     <green> > Open an issue:</green>\n\
     <bold>https://github.com/s2-cli/issues</bold>\n\n\
     <green> > Reach out to us:</green>\n\
     <bold>hi@s2.dev</bold>"
);

const BUG_HELP: &str = color_print::cstr!(
    "\n<cyan><bold>Looks like you may have encountered a bug!</bold></cyan>\n\n\
     <green> > Report this issue here: </green>\n\
     <bold>https://github.com/s2-cli/issues</bold>
"
);

#[derive(Error, Debug, Diagnostic)]
pub enum S2CliError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Config(#[from] S2ConfigError),

    #[error(transparent)]
    #[diagnostic(help("Are you trying to operate on an invalid basin?"))]
    ConvertError(#[from] ConvertError),

    #[error(transparent)]
    #[diagnostic(help("Are you overriding `S2_CLOUD`, `S2_CELL`, or `S2_BASIN_ZONE`?"))]
    HostEndpoints(#[from] ParseError),

    #[error(transparent)]
    #[diagnostic(help("{}", HELP))]
    BasinService(#[from] BasinServiceError),

    #[error(transparent)]
    #[diagnostic(help("{}", HELP))]
    StreamService(#[from] StreamServiceError),

    #[error(transparent)]
    ServiceError(#[from] ServiceError),

    #[error(transparent)]
    #[diagnostic(help("{}", BUG_HELP))]
    InvalidConfig(#[from] serde_json::Error),

    #[error("Failed to initialize a `Record Reader`! {0}")]
    RecordReaderInit(String),

    #[error("Failed to write records: {0}")]
    RecordWrite(String),
}

// Error for holding relevant info from `tonic::Status`
#[derive(Error, Debug, Default)]
#[error("{status}: \n{message}")]
pub struct RequestStatus {
    pub message: String,
    pub status: String,
}

impl From<ClientError> for RequestStatus {
    fn from(error: ClientError) -> Self {
        match error {
            ClientError::Service(status) => Self {
                message: status.message().to_string(),
                status: status.code().to_string(),
            },
            _ => Self {
                message: error.to_string(),
                ..Default::default()
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to {operation} {entity}{plural} {extra}: \n{error}", plural = plural.map_or("", |p| p))]
pub struct ServiceError {
    entity: String,
    operation: String,
    error: RequestStatus,
    extra: String,
    plural: Option<&'static str>,
}

impl ServiceError {
    pub fn new(
        entity: impl Into<String>,
        operation: impl Into<String>,
        error: impl Into<RequestStatus>,
    ) -> Self {
        Self {
            entity: entity.into(),
            operation: operation.into(),
            error: error.into(),
            extra: String::new(),
            plural: None,
        }
    }

    pub fn with_extra(self, extra: impl Into<String>) -> Self {
        Self {
            extra: extra.into(),
            ..self
        }
    }

    pub fn with_plural(self) -> Self {
        let plural = if self.operation.ends_with('s') {
            "es"
        } else {
            "s"
        };
        Self {
            plural: Some(plural),
            ..self
        }
    }
}

pub fn s2_status(error: &ClientError) -> String {
    match error {
        ClientError::Service(status) => status.code().to_string(),
        _ => error.to_string(),
    }
}
