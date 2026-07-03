// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

/// Formatting strategy selected by callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum FormatMode {
    /// Normalize to the project-standard MF2 style.
    #[default]
    Standard,
    /// Preserve source-shape hints where the formatter contract allows it.
    Preserve,
}

/// Typed formatter options.
///
/// CLI, N-API, and WASM adapters validate raw user input before constructing
/// this type, so invalid option states stay outside the Rust formatter core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormatOptions {
    /// Formatting strategy. Defaults to [`FormatMode::Standard`].
    pub mode: FormatMode,
}
