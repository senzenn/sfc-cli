use std::fmt;
use std::path::PathBuf;

/// Main error type for SFC operations
#[derive(Debug)]
pub enum SfcError {
    Io {
        source: std::io::Error,
        context: String,
    },
    Config {
        message: String,
        path: Option<PathBuf>,
    },
    Container {
        name: String,
        operation: String,
        reason: String,
    },
    Package {
        package: String,
        operation: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    Snapshot {
        hash: Option<String>,
        operation: String,
        reason: String,
    },
    System {
        operation: String,
        reason: String,
    },
    Validation {
        field: String,
        value: String,
        reason: String,
    },
    Permission {
        operation: String,
        required: String,
    },
    NotFound {
        resource: String,
        identifier: String,
    },
    AlreadyExists {
        resource: String,
        identifier: String,
    },
    Command {
        command: String,
        exit_code: Option<i32>,
        stderr: String,
    },
    Dependency {
        name: String,
        required_for: String,
        suggestion: Option<String>,
    },
    Generic {
        message: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl fmt::Display for SfcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SfcError::Io { context, .. } => {
                write!(f, "IO error during {}", context)
            }
            SfcError::Config { message, path } => {
                if let Some(path) = path {
                    write!(f, "Configuration error in {}: {}", path.display(), message)
                } else {
                    write!(f, "Configuration error: {}", message)
                }
            }
            SfcError::Container { name, operation, reason } => {
                write!(f, "Container '{}' error during {}: {}", name, operation, reason)
            }
            SfcError::Package { package, operation, .. } => {
                write!(f, "Package '{}' error during {}", package, operation)
            }
            SfcError::Snapshot { hash, operation, reason } => {
                if let Some(hash) = hash {
                    write!(f, "Snapshot '{}' error during {}: {}", hash, operation, reason)
                } else {
                    write!(f, "Snapshot error during {}: {}", operation, reason)
                }
            }
            SfcError::System { operation, reason } => {
                write!(f, "System error during {}: {}", operation, reason)
            }
            SfcError::Validation { field, value, reason } => {
                write!(f, "Validation error for {} '{}': {}", field, value, reason)
            }
            SfcError::Permission { operation, required } => {
                write!(f, "Permission denied for {}: {} required", operation, required)
            }
            SfcError::NotFound { resource, identifier } => {
                write!(f, "{} '{}' not found", resource, identifier)
            }
            SfcError::AlreadyExists { resource, identifier } => {
                write!(f, "{} '{}' already exists", resource, identifier)
            }
            SfcError::Command { command, exit_code, stderr } => {
                if let Some(code) = exit_code {
                    write!(f, "Command '{}' failed with exit code {}: {}", command, code, stderr)
                } else {
                    write!(f, "Command '{}' failed: {}", command, stderr)
                }
            }
            SfcError::Dependency { name, required_for, suggestion } => {
                if let Some(suggestion) = suggestion {
                    write!(f, "Missing dependency '{}' required for {}: {}", name, required_for, suggestion)
                } else {
                    write!(f, "Missing dependency '{}' required for {}", name, required_for)
                }
            }
            SfcError::Generic { message, .. } => {
                write!(f, "{}", message)
            }
        }
    }
}

impl std::error::Error for SfcError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SfcError::Io { source, .. } => Some(source),
            SfcError::Package { source, .. } => Some(source.as_ref()),
            SfcError::Generic { source, .. } => source.as_ref().map(|s| s.as_ref()),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, SfcError>;
pub trait ErrorContext<T> {
    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String;
    
    fn with_io_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String;
}

impl<T> ErrorContext<T> for std::result::Result<T, std::io::Error> {
    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| SfcError::Generic {
            message: f(),
            source: Some(Box::new(e)),
        })
    }
    
    fn with_io_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| SfcError::Io {
            source: e,
            context: f(),
        })
    }
}

impl<T> ErrorContext<T> for std::result::Result<T, SfcError> {
    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| SfcError::Generic {
            message: f(),
            source: Some(Box::new(e)),
        })
    }
    
    fn with_io_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self
    }
}

// Conversion from anyhow::Error for backwards compatibility
impl From<anyhow::Error> for SfcError {
    fn from(err: anyhow::Error) -> Self {
        SfcError::Generic {
            message: err.to_string(),
            source: None,
        }
    }
}

// Conversion to anyhow::Error for backwards compatibility
impl From<SfcError> for anyhow::Error {
    fn from(err: SfcError) -> Self {
        anyhow::anyhow!("{}", err)
    }
}
