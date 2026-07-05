// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Formatter core foundation for `MessageFormat` 2 messages.

mod api;
mod document;
mod error;
mod format;
mod layout;
mod options;
mod render;
mod result;

pub use api::{check_format, check_snapshot, format_message, format_snapshot};
pub use error::{ErrorDetails, ErrorKind, FormatErrorCode, OperationalError};
pub use options::{FormatMode, FormatOptions};
pub use result::{
    FormatCheckResult, FormatCheckSuccess, FormatFailure, FormatResult, FormatSuccess,
};
