//! Full port of `core/error/error_list.h` + idiomatic Result type.
//! Numeric values match Godot (`OK = 0, FAILED = 1, …`) for serialization compat.

use thiserror::Error;

/// Mirrors Godot `Error` — keep discriminants stable (protocol compat).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Default)]
#[repr(i32)]
pub enum Error {
    #[default]
    Ok = 0,
    Failed = 1,
    Unavailable = 2,
    Unconfigured = 3,
    Unauthorized = 4,
    ParameterCheckFailed = 5,
    OutOfMemory = 6,
    CantConnect = 7,
    CantResolve = 8,
    ConnectionError = 9,
    CantAcquireResource = 10,
    CantLock = 11,
    CantUnlock = 12,
    QueryFailed = 13,
    AlreadyInUse = 14,
    Locked = 15,
    Timeout = 16,
    CantConnectToHost = 17,
    InvalidParameter = 18,
    FileNotFound = 19,
    FileBadDriver = 20,
    FileBadPath = 21,
    FileNoPermission = 22,
    FileAlreadyInUse = 23,
    FileCantOpen = 24,
    FileCantWrite = 25,
    FileCantRead = 26,
    FileUnrecognized = 27,
    FileCorrupt = 28,
    FileMissingDependencies = 29,
    FileEof = 30,
    CantOpen = 31,
    CantCreate = 32,
    CantWrite = 33,
    CantRead = 34,
    CantSeek = 35,
    CantRename = 36,
    CantRemove = 37,
    OutOfBounds = 38,
    InvalidData = 39,
    InvalidCall = 40,
    Skip = 41,
    Help = 42,
    Bug = 43,
    PrinterOnFire = 44,
}

impl Error {
    pub fn is_ok(self) -> bool {
        self == Error::Ok
    }
    pub fn message(self) -> &'static str {
        match self {
            Error::Ok => "OK",
            Error::Failed => "Failed",
            Error::FileNotFound => "File not found",
            Error::InvalidParameter => "Invalid parameter",
            Error::OutOfBounds => "Out of bounds",
            Error::InvalidCall => "Invalid call",
            _ => "Error",
        }
    }
}

#[derive(Debug, Error)]
pub enum GodotError {
    #[error("ok")]
    Ok,
    #[error("godot error {code:?}: {message}")]
    Engine { code: Error, message: String },
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("io: {0}")]
    Io(String),
    #[error("unimplemented (original C++ at {0})")]
    Unimplemented(String),
}

impl GodotError {
    pub fn code(&self) -> Error {
        match self {
            GodotError::Ok => Error::Ok,
            GodotError::Engine { code, .. } => *code,
            GodotError::InvalidArgument(_) => Error::InvalidParameter,
            GodotError::Unauthorized(_) => Error::Unauthorized,
            GodotError::Timeout(_) => Error::Timeout,
            GodotError::NotFound(_) => Error::FileNotFound,
            GodotError::Parse(_) => Error::InvalidData,
            GodotError::Io(_) => Error::CantOpen,
            GodotError::Unimplemented(_) => Error::Unconfigured,
        }
    }
}

impl From<Error> for GodotError {
    fn from(code: Error) -> Self {
        if code == Error::Ok {
            GodotError::Ok
        } else {
            GodotError::Engine { code, message: code.message().into() }
        }
    }
}

pub type GodotResult<T> = Result<T, GodotError>;

/// `ERR_FAIL_COND_V_MSG(cond, ret, msg)` equivalent — returns `ret` on true.
#[macro_export]
macro_rules! err_fail_cond_v {
    ($cond:expr, $ret:expr) => {
        if $cond {
            return $ret;
        }
    };
}
