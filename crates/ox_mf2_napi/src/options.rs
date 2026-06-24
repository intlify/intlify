// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use napi_derive::napi;
use ox_mf2_parser::{BatchExecution, BatchParseOptions, ParseOptions, SnapshotOptions};

#[napi(object)]
#[derive(Default)]
pub struct JsParseMessageOptions {
    pub collect_trivia: Option<bool>,
    pub include_trivia: Option<bool>,
    pub include_diagnostics: Option<bool>,
    pub include_source_text: Option<bool>,
}

#[napi(object)]
#[derive(Default)]
pub struct JsParseBatchOptions {
    pub collect_trivia: Option<bool>,
    pub include_trivia: Option<bool>,
    pub include_diagnostics: Option<bool>,
    pub include_source_text: Option<bool>,
    pub batch_execution: Option<String>,
}

#[napi(object)]
pub struct JsDecodeSnapshotOptions {}

impl JsParseMessageOptions {
    pub(crate) fn parse_options(&self) -> ParseOptions {
        ParseOptions {
            recovery: true,
            parse_semantic: false,
            collect_trivia: self.collect_trivia.unwrap_or(true),
        }
    }

    pub(crate) fn snapshot_options(&self) -> SnapshotOptions {
        SnapshotOptions {
            include_diagnostics: self.include_diagnostics.unwrap_or(true),
            include_source_text: self.include_source_text.unwrap_or(false),
            include_trivia: self.include_trivia.unwrap_or(true),
        }
    }
}

impl JsParseBatchOptions {
    pub(crate) fn batch_options(&self) -> BatchParseOptions {
        BatchParseOptions {
            execution: match self.batch_execution.as_deref() {
                Some("parallel") => BatchExecution::Parallel,
                _ => BatchExecution::Sequential,
            },
            max_threads: None,
            preserve_order: true,
            parse: ParseOptions {
                recovery: true,
                parse_semantic: false,
                collect_trivia: self.collect_trivia.unwrap_or(true),
            },
        }
    }

    pub(crate) fn snapshot_options(&self) -> SnapshotOptions {
        SnapshotOptions {
            include_diagnostics: self.include_diagnostics.unwrap_or(true),
            include_source_text: self.include_source_text.unwrap_or(false),
            include_trivia: self.include_trivia.unwrap_or(true),
        }
    }
}
