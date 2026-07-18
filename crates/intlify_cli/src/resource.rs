// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Shared CLI projection for resource-layer identities and failures.

use intlify_resource::{
    DeclaredFormat, EntryKey, ResourceError, ResourceErrorDetails, ResourceErrorSite,
};
use ox_mf2_parser::SourceLocation;
use serde_json::{json, Map, Value};

use crate::error::OperationalError;

pub(crate) fn resource_error(
    path_label: &str,
    source: Option<&str>,
    error: &ResourceError,
) -> OperationalError {
    let details = match error.details() {
        ResourceErrorDetails::FormatUnsupported {
            classification_source,
            declared_format,
            format,
            extension,
            outer_format,
            supported_formats,
        } => {
            let mut details = Map::new();
            details.insert(
                "classificationSource".to_owned(),
                json!(classification_source.as_str()),
            );
            if !matches!(declared_format, DeclaredFormat::Absent) {
                details.insert(
                    "declaredFormat".to_owned(),
                    match declared_format {
                        DeclaredFormat::Absent | DeclaredFormat::Valueless => Value::Null,
                        DeclaredFormat::Value(value) => json!(value.as_ref()),
                    },
                );
            }
            if let Some(format) = format {
                details.insert("format".to_owned(), json!(format));
            }
            details.insert("extension".to_owned(), json!(extension.as_ref()));
            if let Some(outer_format) = outer_format {
                details.insert("outerFormat".to_owned(), json!(outer_format));
            }
            details.insert(
                "supportedFormats".to_owned(),
                json!(supported_formats.as_ref()),
            );
            Value::Object(details)
        }
        ResourceErrorDetails::ParseFailed {
            format,
            outer_format,
        } => {
            let mut details = format_details(format, *outer_format);
            insert_required_site(&mut details, source, error.site());
            Value::Object(details)
        }
        ResourceErrorDetails::EntryUnsupported {
            format,
            outer_format,
            reason,
        } => {
            let mut details = format_details(format, *outer_format);
            details.insert("reason".to_owned(), json!(reason.as_str()));
            insert_required_site(&mut details, source, error.site());
            Value::Object(details)
        }
        ResourceErrorDetails::DocumentUnsupported {
            format,
            outer_format,
            feature,
        } => {
            let mut details = format_details(format, *outer_format);
            details.insert("feature".to_owned(), json!(feature.as_str()));
            insert_required_site(&mut details, source, error.site());
            Value::Object(details)
        }
        ResourceErrorDetails::LimitExceeded {
            resource,
            limit,
            actual,
        } => {
            let mut details = Map::new();
            details.insert("phase".to_owned(), json!(error.phase().as_str()));
            details.insert("resource".to_owned(), json!(resource.as_str()));
            details.insert("limit".to_owned(), exact_counter(*limit));
            details.insert("actual".to_owned(), exact_counter(*actual));
            if let Some(site) = error.site() {
                insert_site(&mut details, source, site);
            }
            Value::Object(details)
        }
        ResourceErrorDetails::Internal { reason } => {
            let mut details = Map::new();
            details.insert("reason".to_owned(), json!(reason.as_str()));
            details.insert("phase".to_owned(), json!(error.phase().as_str()));
            if let Some(key) = error.site().and_then(ResourceErrorSite::entry_key) {
                details.insert("entryKey".to_owned(), entry_key_value(key));
            }
            Value::Object(details)
        }
    };

    OperationalError {
        kind: if error.code().as_str() == "internal_error" {
            "internal"
        } else {
            "input"
        },
        code: error.code().as_str(),
        message: format!("Resource input could not be processed: {path_label}"),
        path: Some(path_label.to_owned()),
        details: Some(details),
    }
}

pub(crate) fn offset_map_error(path_label: &str, key: &EntryKey) -> OperationalError {
    OperationalError {
        kind: "internal",
        code: "internal_error",
        message: format!("A resource diagnostic span could not be mapped: {path_label}"),
        path: Some(path_label.to_owned()),
        details: Some(json!({
            "reason": "resource_offset_map_failed",
            "phase": "map",
            "entryKey": entry_key_value(key)
        })),
    }
}

pub(crate) fn entry_key_value(key: &EntryKey) -> Value {
    json!({
        "path": key.structural_path().as_str(),
        "occurrence": key.occurrence()
    })
}

pub(crate) struct HostLineIndex {
    starts: Vec<u32>,
}

impl HostLineIndex {
    pub(crate) fn new(source: &str) -> Self {
        let mut starts = Vec::with_capacity(source.len() / 32 + 1);
        starts.push(0);
        for (index, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(u32::try_from(index + 1).expect("accepted host bytes fit u32"));
            }
        }
        Self { starts }
    }

    pub(crate) fn diagnostic_location(&self, offset: u32) -> SourceLocation {
        let (line, column) = self.zero_based_location(offset);
        SourceLocation {
            line,
            column: column + 1,
        }
    }

    fn zero_based_location(&self, offset: u32) -> (u32, u32) {
        let line_index = self.starts.partition_point(|start| *start <= offset) - 1;
        let line_start = self.starts[line_index];
        (
            u32::try_from(line_index + 1).expect("accepted host line count fits u32"),
            offset - line_start,
        )
    }
}

fn format_details(format: &'static str, outer_format: Option<&'static str>) -> Map<String, Value> {
    let mut details = Map::new();
    details.insert("format".to_owned(), json!(format));
    if let Some(outer_format) = outer_format {
        details.insert("outerFormat".to_owned(), json!(outer_format));
    }
    details
}

fn insert_required_site(
    details: &mut Map<String, Value>,
    source: Option<&str>,
    site: Option<&ResourceErrorSite>,
) {
    let site = site.expect("resource syntax and support errors always carry a site");
    insert_site(details, source, site);
}

fn insert_site(details: &mut Map<String, Value>, source: Option<&str>, site: &ResourceErrorSite) {
    let source = source.expect("resource processing sites require the original host source");
    let offset = site.span().start();
    assert!(
        usize::try_from(offset)
            .ok()
            .is_some_and(|offset| offset <= source.len()),
        "published resource sites stay within the original host source"
    );
    let (line, column) = one_site_location(source, offset);
    details.insert("offset".to_owned(), json!(offset));
    details.insert("line".to_owned(), json!(line));
    details.insert("column".to_owned(), json!(column));
    if let Some(key) = site.entry_key() {
        details.insert("entryKey".to_owned(), entry_key_value(key));
    }
}

fn exact_counter(value: u128) -> Value {
    Value::Number(
        serde_json::Number::from_u128(value)
            .expect("arbitrary-precision JSON numbers represent every u128 counter"),
    )
}

fn one_site_location(source: &str, offset: u32) -> (u32, u32) {
    let offset = usize::try_from(offset).expect("accepted host offsets fit usize");
    let mut line = 1_u32;
    let mut line_start = 0_usize;
    for (index, byte) in source.as_bytes()[..offset].iter().copied().enumerate() {
        if byte == b'\n' {
            line += 1;
            line_start = index + 1;
        }
    }
    (
        line,
        u32::try_from(offset - line_start).expect("accepted host columns fit u32"),
    )
}

#[cfg(test)]
mod tests {
    use super::exact_counter;

    #[test]
    fn exact_counter_preserves_values_above_u64() {
        let value = u128::from(u64::MAX) + 37;

        assert_eq!(
            serde_json::to_string(&exact_counter(value)).expect("counter should serialize"),
            value.to_string()
        );
    }
}
