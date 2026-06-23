// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use napi::{Error, Status};
use ox_mf2_parser::{DecodeError, SnapshotWriteError};

pub(crate) fn not_implemented(api: &str) -> Error {
    Error::new(
        Status::GenericFailure,
        format!("{api} is reserved for the Phase 2 snapshot-backed API implementation"),
    )
}

pub(crate) fn snapshot_write(error: SnapshotWriteError) -> Error {
    Error::new(Status::GenericFailure, error.to_string())
}

pub(crate) fn decode(error: DecodeError) -> Error {
    Error::new(Status::InvalidArg, error.to_string())
}
