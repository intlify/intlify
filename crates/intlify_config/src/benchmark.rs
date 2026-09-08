// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Isolated observational measurement support for the minimum 015 path.
//! No product resolver or partial Profile is exposed through this module.

mod quantity;

#[cfg(feature = "benchmark")]
mod build;
#[cfg(feature = "benchmark")]
mod cases;
#[cfg(feature = "benchmark")]
mod clock;
#[cfg(feature = "benchmark")]
mod collect;
#[cfg(feature = "benchmark")]
mod context;
#[cfg(feature = "benchmark")]
mod descriptor;
#[cfg(feature = "benchmark")]
mod environment;
#[cfg(feature = "benchmark")]
mod inventory;
#[cfg(feature = "benchmark")]
mod locale;
#[cfg(feature = "benchmark")]
mod locale_core;
#[cfg(feature = "benchmark")]
mod measure;
#[cfg(feature = "benchmark")]
pub(crate) mod observation;
#[cfg(feature = "benchmark")]
mod operation;
#[cfg(feature = "benchmark")]
mod profile;
#[cfg(feature = "benchmark")]
mod run;
#[cfg(feature = "benchmark")]
mod sample;
#[cfg(feature = "benchmark")]
mod shared;
#[cfg(feature = "benchmark")]
mod work;

#[cfg(feature = "benchmark")]
pub(crate) fn run_plan_schema() -> Result<serde_json::Value, serde_json::Error> {
    shared::plan::record_schema()
}

#[cfg(feature = "benchmark")]
pub(crate) fn measurement_case_schema() -> Result<serde_json::Value, serde_json::Error> {
    shared::plan::case_schema()
}

#[cfg(all(test, feature = "benchmark"))]
mod inventory_tests;
#[cfg(all(test, feature = "benchmark"))]
mod profile_tests;
