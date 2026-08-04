// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Deterministic opaque logical paths for built-in ESM artifacts.
//!
//! Locale and scope identities are framed with the representation domain and
//! format version before standard unkeyed BLAKE3-256 hashing. The resulting
//! portable paths expose no reversible locale or scope spelling.

use std::mem::size_of;

use intlify_contract::Locale;
use intlify_linker::ResolvedCatalogScopeId;

use crate::{ExportArtifactPath, ExportError};

const FORMAT_MAJOR: u16 = 0;
const FORMAT_MINOR: u16 = 1;

// Capacity accounting mirrors the exact frame layout: one NUL terminator after
// the domain, two big-endian u16 version fields, and a length prefix before
// every variable-width identity component.
const FRAME_DOMAIN_TERMINATOR: u8 = 0;
const FRAME_DOMAIN_TERMINATOR_BYTES: usize = size_of::<u8>();
const FORMAT_VERSION_BYTES: usize = size_of::<u16>() + size_of::<u16>();
const U16_LENGTH_PREFIX_BYTES: usize = size_of::<u16>();
const U32_LENGTH_PREFIX_BYTES: usize = size_of::<u32>();

pub(super) fn locale_path(locale: &Locale) -> Result<ExportArtifactPath, ExportError> {
    let locale_bytes = locale.as_str().as_bytes();
    let locale_len =
        u32::try_from(locale_bytes.len()).map_err(|_| super::artifact_assembly_error())?;
    let mut framed = Vec::with_capacity(
        b"dev.intlify/esm-locale-path".len()
            + FRAME_DOMAIN_TERMINATOR_BYTES
            + FORMAT_VERSION_BYTES
            + U32_LENGTH_PREFIX_BYTES
            + locale_bytes.len(),
    );
    framed.extend_from_slice(b"dev.intlify/esm-locale-path");
    framed.push(FRAME_DOMAIN_TERMINATOR);
    framed.extend_from_slice(&FORMAT_MAJOR.to_be_bytes());
    framed.extend_from_slice(&FORMAT_MINOR.to_be_bytes());
    framed.extend_from_slice(&locale_len.to_be_bytes());
    framed.extend_from_slice(locale_bytes);
    let filename = format!("locale-{}.mjs", blake3::hash(&framed).to_hex());
    checked_path(["locales".into(), filename.into_boxed_str()])
}

pub(super) fn accessor_path(
    scope: &ResolvedCatalogScopeId,
) -> Result<ExportArtifactPath, ExportError> {
    let scope = scope.as_catalog_scope();
    accessor_path_from_parts(scope.namespace().as_str(), scope.name().as_str())
}

fn accessor_path_from_parts(
    namespace: &str,
    scope_name: &str,
) -> Result<ExportArtifactPath, ExportError> {
    let namespace = namespace.as_bytes();
    let scope_name = scope_name.as_bytes();
    let namespace_len =
        u16::try_from(namespace.len()).map_err(|_| super::artifact_assembly_error())?;
    let scope_name_len =
        u16::try_from(scope_name.len()).map_err(|_| super::artifact_assembly_error())?;
    let mut framed = Vec::with_capacity(
        b"dev.intlify/typescript-accessor-path".len()
            + FRAME_DOMAIN_TERMINATOR_BYTES
            + FORMAT_VERSION_BYTES
            + U16_LENGTH_PREFIX_BYTES
            + namespace.len()
            + U16_LENGTH_PREFIX_BYTES
            + scope_name.len(),
    );
    framed.extend_from_slice(b"dev.intlify/typescript-accessor-path");
    framed.push(FRAME_DOMAIN_TERMINATOR);
    framed.extend_from_slice(&FORMAT_MAJOR.to_be_bytes());
    framed.extend_from_slice(&FORMAT_MINOR.to_be_bytes());
    framed.extend_from_slice(&namespace_len.to_be_bytes());
    framed.extend_from_slice(namespace);
    framed.extend_from_slice(&scope_name_len.to_be_bytes());
    framed.extend_from_slice(scope_name);
    let filename = format!("scope-{}.ts", blake3::hash(&framed).to_hex());
    checked_path(["accessors".into(), filename.into_boxed_str()])
}

pub(super) fn loader_path() -> Result<ExportArtifactPath, ExportError> {
    checked_path([Box::<str>::from("loader.mjs")])
}

pub(super) fn relative_module_specifier(path: &ExportArtifactPath) -> String {
    let mut specifier = String::from("./");
    for (index, segment) in path.segments().iter().enumerate() {
        if index != 0 {
            specifier.push('/');
        }
        specifier.push_str(segment.as_str());
    }
    specifier
}

fn checked_path<const N: usize>(
    segments: [Box<str>; N],
) -> Result<ExportArtifactPath, ExportError> {
    ExportArtifactPath::try_new(segments).map_err(|_| super::artifact_assembly_error())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use intlify_contract::{ArtifactNamespace, CatalogScopeId, CatalogScopeName, Locale};
    use intlify_linker::{
        CoverageBaseline, DeliveryUnitGraph, DynamicReferenceMode, InputCompleteness, LinkPolicy,
        LinkRequest, PlacementPolicy, ResolvedCatalogScopeId, ScopeCompleteness,
        ScopeCompletenessTable, ScopeMappingTable,
    };

    use super::{accessor_path, accessor_path_from_parts, locale_path};

    #[test]
    fn locale_path_uses_the_exact_opaque_framing() {
        let path = locale_path(&Locale::try_new("en").unwrap()).unwrap();
        assert_eq!(path.segments()[0].as_str(), "locales");
        assert_eq!(
            path.segments()[1].as_str(),
            "locale-2ad8166799cc20f0f848091b69d83f4c07244462262297fe9854903988443387.mjs"
        );
    }

    #[test]
    fn accessor_path_uses_the_exact_opaque_framing() {
        let resolved = resolved_scope("app");
        let path = accessor_path(&resolved).unwrap();
        assert_eq!(path.segments()[0].as_str(), "accessors");
        assert_eq!(
            path.segments()[1].as_str(),
            "scope-73907d636a7fe439dc88556cf27161097f869fdafbe02bd38ffb2ced211f2578.ts"
        );
    }

    #[test]
    fn locale_paths_preserve_case_unicode_normalization_controls_and_boundaries() {
        let locales = [
            "a".to_owned(),
            "A".to_owned(),
            "é".to_owned(),
            "e\u{0301}".to_owned(),
            " ".to_owned(),
            "\u{0001}".to_owned(),
            "a".repeat(255),
        ];
        let paths = locales
            .iter()
            .map(|value| {
                let path = locale_path(&Locale::try_new(value.clone()).unwrap()).unwrap();
                assert_eq!(path.segments()[1].as_str().len(), 75);
                assert!(path.segments()[1].as_str()[7..71]
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
                path
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(paths.len(), locales.len());
        assert!(Locale::try_new("").is_err());
    }

    #[test]
    fn accessor_paths_preserve_exact_scope_identity_at_the_byte_boundary() {
        let scope_names = [
            "a".to_owned(),
            "A".to_owned(),
            "é".to_owned(),
            "e\u{0301}".to_owned(),
            "a".repeat(255),
        ];
        let paths = scope_names
            .iter()
            .map(|name| {
                let path = accessor_path(&resolved_scope(name)).unwrap();
                assert_eq!(path.segments()[1].as_str().len(), 73);
                assert!(path.segments()[1].as_str()[6..70]
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
                path
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(paths.len(), scope_names.len());

        // Both pairs concatenate to "project-abc". The u16 length prefix on
        // each component keeps the namespace/name split part of the hash input.
        let namespace_ends_earlier = accessor_path_from_parts("project-a", "bc").unwrap();
        let namespace_ends_later = accessor_path_from_parts("project-ab", "c").unwrap();
        assert_ne!(namespace_ends_earlier, namespace_ends_later);
    }

    fn resolved_scope(name: &str) -> ResolvedCatalogScopeId {
        let scope = CatalogScopeId::new(
            ArtifactNamespace::Project,
            CatalogScopeName::try_new(name).unwrap(),
        );
        let locale = Locale::try_new("en").unwrap();
        let limits = intlify_contract::LinkLimits::default();
        let policy = LinkPolicy::try_new(
            vec![locale.clone()],
            Vec::new(),
            Vec::new(),
            vec![CoverageBaseline::new(scope.clone(), locale)],
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &limits,
        )
        .unwrap();
        let mappings = ScopeMappingTable::empty(std::slice::from_ref(&scope), &limits).unwrap();
        let completeness = ScopeCompletenessTable::try_new(
            std::slice::from_ref(&scope),
            vec![ScopeCompleteness::try_new(
                scope.clone(),
                InputCompleteness::Closed,
                InputCompleteness::Closed,
            )
            .unwrap()],
            &limits,
        )
        .unwrap();
        let definitions = Vec::new();
        let references = Vec::new();
        let graph = DeliveryUnitGraph::single_main(&limits).unwrap();
        let request = LinkRequest::try_new(
            &references,
            &definitions,
            &policy,
            &mappings,
            &completeness,
            &graph,
            &limits,
        )
        .unwrap();
        intlify_linker::link(&request).unwrap().typed_key_models()[0]
            .resolved_scope()
            .clone()
    }
}
