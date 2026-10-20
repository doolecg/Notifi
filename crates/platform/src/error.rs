use crate::caps::Feature;

pub type Result<T> = std::result::Result<T, PlatformError>;

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("{} is not supported here: {reason}", feature.name())]
    Unsupported { feature: Feature, reason: String },
    #[error("permission needed: {what}")]
    PermissionDenied { what: String },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("{context}: {message}")]
    Backend { context: &'static str, message: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl PlatformError {
    pub fn unsupported(feature: Feature, reason: impl Into<String>) -> Self {
        Self::Unsupported { feature, reason: reason.into() }
    }

    pub(crate) fn not_implemented(feature: Feature) -> Self {
        Self::unsupported(feature, "not implemented yet")
    }

    pub(crate) fn backend(context: &'static str, message: impl ToString) -> Self {
        Self::Backend { context, message: message.to_string() }
    }
}

#[cfg(target_os = "linux")]
impl From<zbus::Error> for PlatformError {
    fn from(e: zbus::Error) -> Self {
        Self::backend("dbus", e)
    }
}

#[cfg(target_os = "linux")]
impl From<zbus::fdo::Error> for PlatformError {
    fn from(e: zbus::fdo::Error) -> Self {
        Self::backend("dbus", e)
    }
}
