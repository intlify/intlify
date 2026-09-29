// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Which imports are authoring intrinsics.
//!
//! 016 recognizes `intent`, `mf2` and `noIntent` by the binding a name
//! resolves to, never by how the name is spelled. A profile registers exact
//! module exports as intrinsics. A unit's named import of one of them, under
//! whatever local name, binds that intrinsic, and every use of the local
//! binding is a use of the intrinsic. A function that merely shares the name,
//! a parameter that shadows it, or a local variable of the same name is not.
//!
//! The registered module is compared with an import's specifier exactly.
//! Nothing here resolves packages, follows re-exports, or reads other files.
//!
//! Only direct named imports are supported. An import of a registered module
//! in any other form is reported once, where it is written: a default or
//! namespace import, a dynamic `import()` with a literal specifier, a
//! TypeScript `import = require()`, and a re-export. A re-export hands the
//! intrinsics to other modules under a specifier nobody registered, where
//! they would be read as ordinary calls; reporting it here, where it is known
//! to be an intrinsic, is what keeps those uses from going silently unread.

use std::collections::BTreeMap;

use intlify_authoring::ReasonFamily;
use oxc_ast::ast::{
    Declaration, ImportDeclarationSpecifier, ImportOrExportKind, Program, Statement,
    TSModuleReference,
};
use oxc_semantic::SymbolId;
use oxc_span::GetSpan;

use crate::detail;
use crate::failure::ProducerFailure;
use crate::report::Reporter;

/// One of the three authoring intrinsics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Intrinsic {
    /// `intent(source, parameters?)`: a message and its use site.
    Intent,
    /// `` mf2`...` ``: a reusable message declaration.
    Mf2,
    /// `noIntent(value, reason)`: a value excluded from localization.
    NoIntent,
}

/// One registered module export that is an authoring intrinsic.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IntrinsicBinding {
    module: String,
    export: String,
    intrinsic: Intrinsic,
}

impl IntrinsicBinding {
    /// Register `export` of `module` as `intrinsic`.
    ///
    /// Nothing is checked here. A profile checks the whole set together,
    /// because a conflict is a property of the set, not of one binding.
    #[must_use]
    pub fn new(module: &str, export: &str, intrinsic: Intrinsic) -> Self {
        Self {
            module: module.to_owned(),
            export: export.to_owned(),
            intrinsic,
        }
    }

    /// Borrow the exact module specifier.
    #[must_use]
    pub fn module(&self) -> &str {
        &self.module
    }

    /// Borrow the exact export name.
    #[must_use]
    pub fn export(&self) -> &str {
        &self.export
    }

    /// Return which intrinsic the export is.
    #[must_use]
    pub const fn intrinsic(&self) -> Intrinsic {
        self.intrinsic
    }
}

/// Why a binding set cannot be used.
///
/// These are mistakes in the caller's configuration, not in source, so a
/// profile refuses to be built from them rather than reading source under
/// rules that contradict themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingError {
    /// A binding names an empty module specifier.
    EmptyModule,
    /// A binding names an empty export.
    EmptyExport,
    /// The same module export is registered twice.
    Duplicate {
        /// The module specifier.
        module: String,
        /// The export registered twice.
        export: String,
    },
    /// One module export is registered as two different intrinsics.
    Conflict {
        /// The module specifier.
        module: String,
        /// The export registered as two intrinsics.
        export: String,
    },
}

/// A checked binding set, ordered by module and export.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Bindings {
    entries: Box<[IntrinsicBinding]>,
}

impl Bindings {
    /// Check a binding set and put it in order.
    pub(crate) fn new(
        bindings: impl IntoIterator<Item = IntrinsicBinding>,
    ) -> Result<Self, BindingError> {
        let mut entries: Vec<IntrinsicBinding> = bindings.into_iter().collect();
        for binding in &entries {
            if binding.module.is_empty() {
                return Err(BindingError::EmptyModule);
            }
            if binding.export.is_empty() {
                return Err(BindingError::EmptyExport);
            }
        }
        entries.sort();
        for pair in entries.windows(2) {
            if (&pair[0].module, &pair[0].export) != (&pair[1].module, &pair[1].export) {
                continue;
            }
            let (module, export) = (pair[0].module.clone(), pair[0].export.clone());
            return Err(if pair[0].intrinsic == pair[1].intrinsic {
                BindingError::Duplicate { module, export }
            } else {
                BindingError::Conflict { module, export }
            });
        }
        Ok(Self {
            entries: entries.into_boxed_slice(),
        })
    }

    /// Borrow the bindings in order.
    pub(crate) fn entries(&self) -> &[IntrinsicBinding] {
        &self.entries
    }

    /// Return whether any binding is registered.
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return whether `module` is a registered module specifier.
    pub(crate) fn registers(&self, module: &str) -> bool {
        self.entries.iter().any(|binding| binding.module == module)
    }

    /// Return the intrinsic `export` of `module` is registered as.
    pub(crate) fn find(&self, module: &str, export: &str) -> Option<Intrinsic> {
        self.entries
            .binary_search_by(|binding| {
                (binding.module.as_str(), binding.export.as_str()).cmp(&(module, export))
            })
            .ok()
            .map(|index| self.entries[index].intrinsic)
    }
}

/// The local bindings of one unit that are authoring intrinsics.
#[derive(Debug, Default)]
pub(crate) struct IntrinsicSymbols {
    symbols: BTreeMap<SymbolId, Intrinsic>,
}

impl IntrinsicSymbols {
    /// Return the intrinsic `symbol` binds, if it binds one.
    pub(crate) fn get(&self, symbol: SymbolId) -> Option<Intrinsic> {
        self.symbols.get(&symbol).copied()
    }
}

/// Find the unit's intrinsic bindings, reporting unsupported import forms.
///
/// Imports are only ever top-level statements, so this reads the program body
/// and nothing below it.
pub(crate) fn scan(
    program: &Program<'_>,
    bindings: &Bindings,
    reporter: &mut Reporter,
) -> Result<IntrinsicSymbols, ProducerFailure> {
    let mut found = IntrinsicSymbols::default();
    for statement in &program.body {
        match statement {
            Statement::ImportDeclaration(declaration) => {
                let module = declaration.source.value.as_str();
                // A type-only import binds no value, so it cannot bind one.
                if declaration.import_kind == ImportOrExportKind::Type
                    || !bindings.registers(module)
                {
                    continue;
                }
                for specifier in declaration.specifiers.iter().flatten() {
                    match specifier {
                        ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                            if specifier.import_kind == ImportOrExportKind::Type {
                                continue;
                            }
                            // An unregistered name from a registered module is not a
                            // mistake: the same package may export ordinary runtime API.
                            let Some(intrinsic) = bindings.find(module, &specifier.imported.name())
                            else {
                                continue;
                            };
                            if let Some(symbol) = specifier.local.symbol_id.get() {
                                found.symbols.insert(symbol, intrinsic);
                            }
                        }
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
                            unsupported_import(reporter, specifier.span)?;
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
                            unsupported_import(reporter, specifier.span)?;
                        }
                    }
                }
            }
            Statement::ExportNamedDeclaration(declaration) => {
                if declaration.export_kind == ImportOrExportKind::Type {
                    continue;
                }
                if let Some(source) = &declaration.source {
                    let module = source.value.as_str();
                    for specifier in &declaration.specifiers {
                        if specifier.export_kind != ImportOrExportKind::Type
                            && bindings.find(module, &specifier.local.name()).is_some()
                        {
                            unsupported_import(reporter, specifier.span)?;
                        }
                    }
                }
                if let Some(Declaration::TSImportEqualsDeclaration(import)) =
                    &declaration.declaration
                {
                    required(import, bindings, reporter)?;
                }
            }
            Statement::ExportAllDeclaration(declaration) => {
                if declaration.export_kind != ImportOrExportKind::Type
                    && bindings.registers(declaration.source.value.as_str())
                {
                    unsupported_import(reporter, declaration.span)?;
                }
            }
            Statement::TSImportEqualsDeclaration(import) => required(import, bindings, reporter)?,
            _ => {}
        }
    }
    Ok(found)
}

/// Report a TypeScript `import = require()` of a registered module.
fn required(
    import: &oxc_ast::ast::TSImportEqualsDeclaration<'_>,
    bindings: &Bindings,
    reporter: &mut Reporter,
) -> Result<(), ProducerFailure> {
    if import.import_kind == ImportOrExportKind::Type {
        return Ok(());
    }
    if let TSModuleReference::ExternalModuleReference(reference) = &import.module_reference {
        if bindings.registers(reference.expression.value.as_str()) {
            unsupported_import(reporter, import.span())?;
        }
    }
    Ok(())
}

fn unsupported_import(
    reporter: &mut Reporter,
    span: oxc_span::Span,
) -> Result<(), ProducerFailure> {
    reporter.at(
        ReasonFamily::AuthoringFormUnsupported,
        detail::import_form_unsupported(),
        span,
    )
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{AuthoringFailure, Diagnostic, LimitKind, Location};
    use oxc_allocator::Allocator;

    use super::*;
    use crate::grammar::Grammar;
    use crate::test_support::{at, bindings, parse, reporter, snapshot, UNIT};

    fn binding(module: &str, export: &str, intrinsic: Intrinsic) -> IntrinsicBinding {
        IntrinsicBinding::new(module, export, intrinsic)
    }

    /// What scanning one unit found: each top-level binding that is an
    /// intrinsic, by name, and each reported import, as a detail and range.
    type Scanned = (Vec<(String, Intrinsic)>, Vec<(&'static str, (u64, u64))>);

    fn scanned(grammar: Grammar, text: &str) -> Scanned {
        let arena = Allocator::default();
        let parsed = parse(&arena, text, grammar);
        let mut reporter = reporter(text);
        let symbols = scan(parsed.program, &bindings(), &mut reporter).expect("the scan runs");
        let scoping = parsed.semantic.scoping();
        let mut found: Vec<(String, Intrinsic)> = scoping
            .symbol_ids()
            .filter_map(|symbol| {
                symbols
                    .get(symbol)
                    .map(|intrinsic| (scoping.symbol_name(symbol).to_owned(), intrinsic))
            })
            .collect();
        found.sort();
        let reported = reporter
            .into_diagnostics()
            .iter()
            .map(|record| (detail_of(record), range_of(record)))
            .collect();
        (found, reported)
    }

    fn detail_of(record: &Diagnostic) -> &'static str {
        assert_eq!(record.origin().code(), "authoring-form-unsupported");
        record.detail().expect("a detail").as_str()
    }

    fn range_of(record: &Diagnostic) -> (u64, u64) {
        let Location::Region(region) = record.location() else {
            panic!("an import is reported where it is written");
        };
        (region.range().start(), region.range().end())
    }

    #[test]
    fn a_binding_set_is_ordered_and_found_by_its_exact_module_and_export() {
        let bindings = Bindings::new([
            binding("fixture-authoring", "noIntent", Intrinsic::NoIntent),
            binding("fixture-authoring", "intent", Intrinsic::Intent),
            binding("fixture-authoring", "mf2", Intrinsic::Mf2),
            // Two names for one intrinsic are allowed; a package may alias it.
            binding("fixture-authoring", "t", Intrinsic::Intent),
        ])
        .unwrap();
        let exports: Vec<&str> = bindings
            .entries()
            .iter()
            .map(IntrinsicBinding::export)
            .collect();
        assert_eq!(exports, ["intent", "mf2", "noIntent", "t"]);
        assert_eq!(
            bindings.find("fixture-authoring", "t"),
            Some(Intrinsic::Intent)
        );
        assert_eq!(bindings.find("fixture-authoring", "Intent"), None);
        assert_eq!(bindings.find("fixture-authoring/", "intent"), None);
        assert!(bindings.registers("fixture-authoring"));
        assert!(!bindings.registers("Fixture-authoring"));
    }

    #[test]
    fn a_binding_set_that_contradicts_itself_is_refused() {
        assert_eq!(
            Bindings::new([binding("", "intent", Intrinsic::Intent)]),
            Err(BindingError::EmptyModule)
        );
        assert_eq!(
            Bindings::new([binding("fixture-authoring", "", Intrinsic::Intent)]),
            Err(BindingError::EmptyExport)
        );
        assert_eq!(
            Bindings::new([
                binding("fixture-authoring", "intent", Intrinsic::Intent),
                binding("fixture-authoring", "intent", Intrinsic::Intent),
            ]),
            Err(BindingError::Duplicate {
                module: "fixture-authoring".into(),
                export: "intent".into()
            })
        );
        assert_eq!(
            Bindings::new([
                binding("fixture-authoring", "intent", Intrinsic::Intent),
                binding("fixture-authoring", "intent", Intrinsic::Mf2),
            ]),
            Err(BindingError::Conflict {
                module: "fixture-authoring".into(),
                export: "intent".into()
            })
        );
        // The same export name under two modules is two different exports.
        assert!(Bindings::new([
            binding("fixture-authoring", "intent", Intrinsic::Intent),
            binding("other-authoring", "intent", Intrinsic::Mf2),
        ])
        .is_ok());
        assert!(Bindings::new([]).unwrap().is_empty());
    }

    #[test]
    fn a_registered_binding_answers_for_its_module_export_and_intrinsic() {
        let registered = binding("fixture-authoring", "t", Intrinsic::Intent);
        assert_eq!(registered.module(), "fixture-authoring");
        assert_eq!(registered.export(), "t");
        assert_eq!(registered.intrinsic(), Intrinsic::Intent);
    }

    #[test]
    fn a_direct_named_import_binds_its_local_name_whatever_it_is() {
        let text =
            "import { intent as t, mf2, noIntent as skip, format } from 'fixture-authoring'\n\
                    import { intent } from 'other-authoring'\n";
        let (found, reported) = scanned(Grammar::JsModule, text);
        assert_eq!(
            found,
            [
                ("mf2".to_owned(), Intrinsic::Mf2),
                ("skip".to_owned(), Intrinsic::NoIntent),
                ("t".to_owned(), Intrinsic::Intent),
            ]
        );
        // `format` is an ordinary export of the same module, and `intent`
        // comes from a module nobody registered: neither is a mistake.
        assert_eq!(reported, []);
    }

    #[test]
    fn a_type_only_import_or_export_binds_no_value() {
        let text = "import type { intent } from 'fixture-authoring'\n\
                    import { type mf2 } from 'fixture-authoring'\n\
                    export type { noIntent } from 'fixture-authoring'\n\
                    export { type intent as t } from 'fixture-authoring'\n\
                    import 'fixture-authoring'\n";
        assert_eq!(scanned(Grammar::TsModule, text), (vec![], vec![]));
    }

    #[test]
    fn each_other_form_importing_a_registered_module_is_reported_where_it_is_written() {
        let text = "import authoring, { mf2 } from 'fixture-authoring'\n\
                    import * as all from 'fixture-authoring'\n\
                    export { intent as localize } from 'fixture-authoring'\n\
                    export * as everything from 'fixture-authoring'\n\
                    export * from 'fixture-authoring'\n";
        let (found, reported) = scanned(Grammar::JsModule, text);
        // The named specifier beside a reported default one still binds.
        assert_eq!(found, [("mf2".to_owned(), Intrinsic::Mf2)]);
        let (default, _) = at(text, "authoring,");
        assert_eq!(
            reported,
            [
                ("import-form-unsupported", (default, default + 9)),
                ("import-form-unsupported", at(text, "* as all")),
                ("import-form-unsupported", at(text, "intent as localize")),
                (
                    "import-form-unsupported",
                    at(text, "export * as everything from 'fixture-authoring'")
                ),
                (
                    "import-form-unsupported",
                    at(text, "export * from 'fixture-authoring'")
                ),
            ]
        );
    }

    #[test]
    fn a_typescript_import_require_of_a_registered_module_is_reported() {
        let text = "import x = require('fixture-authoring')\n\
                    export import y = require('fixture-authoring')\n\
                    import z = require('other-authoring')\n\
                    import type w = require('fixture-authoring')\n\
                    namespace N { export const v = 1 }\n\
                    import alias = N.v\n";
        let (found, reported) = scanned(Grammar::TsModule, text);
        assert_eq!(found, []);
        // An exported one is reported at the declaration after `export`.
        assert_eq!(
            reported,
            [
                (
                    "import-form-unsupported",
                    at(text, "import x = require('fixture-authoring')")
                ),
                (
                    "import-form-unsupported",
                    at(text, "import y = require('fixture-authoring')")
                ),
            ]
        );
    }

    #[test]
    fn re_exporting_what_is_not_an_intrinsic_is_ordinary() {
        let text = "export { format } from 'fixture-authoring'\n\
                    export * from 'other-authoring'\n\
                    export { intent } from 'other-authoring'\n";
        assert_eq!(scanned(Grammar::JsModule, text), (vec![], vec![]));
    }

    #[test]
    fn a_report_with_no_room_left_stops_the_scan() {
        let text = "import * as all from 'fixture-authoring'\n";
        let arena = Allocator::default();
        let parsed = parse(&arena, text, Grammar::JsModule);
        let mut full = Reporter::new(snapshot(UNIT, Grammar::JsModule, text.as_bytes()), 0);
        assert_eq!(
            scan(parsed.program, &bindings(), &mut full).map(|_| ()),
            Err(ProducerFailure::Authoring(AuthoringFailure::Limit(
                LimitKind::Diagnostics
            )))
        );
    }
}
