// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum FormatMode {
    #[default]
    Standard,
    Preserve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormatOptions {
    pub mode: FormatMode,
}
