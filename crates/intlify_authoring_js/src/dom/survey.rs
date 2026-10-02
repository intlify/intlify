// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What one unit says about DOM receivers before any path is followed.
//!
//! The survey reads the whole unit once and records what does not depend on
//! control flow: which calls are admitted origins, which `const` bindings
//! hold them and through how many aliases, which functions capture them,
//! which functions are dynamically scoped, and every `textContent`
//! assignment with what its receiver and value are. Whether a receiver's
//! evidence still holds where it is assigned is the flow analysis's
//! question, asked only of the functions that need it.

use std::collections::{BTreeMap, BTreeSet};

use intlify_authoring::Detail;
use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, AssignmentExpression, AssignmentOperator, AssignmentTarget,
    BindingPattern, CallExpression, Class, Declaration, ExportNamedDeclaration, Expression,
    Function, IdentifierReference, Program, TSModuleDeclaration, VariableDeclarationKind,
    VariableDeclarator, WithStatement,
};
use oxc_ast_visit::{walk, Visit};
use oxc_semantic::{ScopeFlags, Scoping, SymbolId};
use oxc_span::{GetSpan, Span};

use crate::binding::{Intrinsic, IntrinsicSymbols};
use crate::cooked::{cook_string, cook_template, CookFailure, Cooked};
use crate::detail;
use crate::limits::JsAuthoringLimits;
use crate::syntax::{static_string, transparent};

/// A function-like scope, or the program, by its span.
pub(super) type Key = (u32, u32);

pub(super) const fn key(span: Span) -> Key {
    (span.start, span.end)
}

/// Why a receiver with a known DOM origin has no evidence to follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Unestablished {
    /// It is reached through `let`, `var`, a script's top level, or an alias
    /// made in another function.
    Binding,
    /// It is used in another function than the one that made it.
    Captured,
    /// Its origin call does not take exactly one static string.
    Argument,
    /// Its origin is in a dynamically scoped function.
    DynamicScope,
}

impl Unestablished {
    pub(super) fn detail(self) -> Detail {
        match self {
            Self::Binding => detail::receiver_binding_unsupported(),
            Self::Captured => detail::receiver_captured(),
            Self::Argument => detail::origin_argument_unsupported(),
            Self::DynamicScope => detail::origin_scope_dynamic(),
        }
    }
}

/// What a binding holding a DOM receiver is known to be.
#[derive(Debug, Clone, Copy)]
pub(super) enum Tracking {
    /// A `const` followed from an admitted origin, `depth` aliases away.
    Tracked {
        origin: usize,
        root: Key,
        depth: u64,
    },
    /// A binding with a known origin the tracer does not follow.
    Unestablished(Unestablished),
    /// An alias past the bound on alias chains.
    AliasChain,
}

/// The receiver of one `textContent` assignment.
#[derive(Debug, Clone, Copy)]
pub(super) enum Receiver {
    /// The origin call itself, which needs no path to hold.
    Direct { root: Key },
    /// A followed binding in the same function.
    Tracked { origin: usize },
    /// A known origin without evidence to follow.
    Unestablished(Unestablished),
    /// An alias past the bound on alias chains.
    AliasChain,
}

/// The value of one `textContent` assignment.
#[derive(Debug)]
pub(super) enum Value {
    /// A string or a template without substitutions, decoded.
    Literal {
        span: Span,
        cooked: Result<Cooked, CookFailure>,
    },
    /// An `mf2` tag, which declares a message rather than being one to show.
    Descriptor(Span),
    /// Anything else.
    Dynamic(Span),
}

/// One `textContent` assignment to a receiver with a known DOM origin.
#[derive(Debug)]
pub(super) struct Candidate {
    pub(super) assignment: Span,
    pub(super) receiver: Receiver,
    pub(super) value: Value,
    pub(super) compound: bool,
}

/// One admitted origin call.
#[derive(Debug, Clone, Copy)]
pub(super) struct Origin {
    pub(super) root: Key,
    /// Whether each call makes a new element, as `document.createElement()`
    /// does. A query can find an element some earlier code already reached.
    pub(super) fresh: bool,
}

/// Everything the survey found.
#[derive(Debug, Default)]
pub(super) struct Survey {
    pub(super) origins: Vec<Origin>,
    pub(super) bindings: BTreeMap<SymbolId, Tracking>,
    /// The origins each function-like scope refers to from inside it.
    pub(super) captures: BTreeMap<Key, BTreeSet<usize>>,
    /// The origins captured by function declarations each scope owns, at
    /// any block depth, which take effect where the scope starts.
    pub(super) hoisted: BTreeMap<Key, BTreeSet<usize>>,
    /// Each function-like scope's enclosing one.
    pub(super) parents: BTreeMap<Key, Key>,
    /// Scopes containing a `with` statement or a sloppy direct `eval`.
    pub(super) dynamic: BTreeSet<Key>,
    /// Bindings a module exports, which other modules can reach.
    pub(super) exported: BTreeSet<SymbolId>,
    pub(super) candidates: Vec<Candidate>,
    /// Literal `textContent` assignments to receivers with no known origin.
    pub(super) outside_profile: u64,
}

impl Survey {
    /// Return whether a scope or any scope around it is dynamically scoped.
    pub(super) fn is_dynamic(&self, mut scope: Key) -> bool {
        loop {
            if self.dynamic.contains(&scope) {
                return true;
            }
            match self.parents.get(&scope) {
                Some(&parent) => scope = parent,
                None => return false,
            }
        }
    }
}

/// Survey one unit.
pub(super) fn survey(
    program: &Program<'_>,
    text: &str,
    script: bool,
    scoping: &Scoping,
    intrinsics: &IntrinsicSymbols,
    limits: &JsAuthoringLimits,
) -> Survey {
    let mut surveyor = Surveyor {
        text,
        script,
        scoping,
        intrinsics,
        limits,
        stack: Vec::new(),
        declarations: Vec::new(),
        declaring: true,
        found: Survey::default(),
    };
    // Every binding is known before any use is read: a function written
    // before a declaration still reaches it once it runs.
    surveyor.visit_program(program);
    surveyor.declaring = false;
    surveyor.visit_program(program);
    for (owner, declared) in std::mem::take(&mut surveyor.declarations) {
        if let Some(captured) = surveyor.found.captures.get(&declared).cloned() {
            surveyor
                .found
                .hoisted
                .entry(owner)
                .or_default()
                .extend(captured);
        }
    }
    surveyor.found
}

struct Surveyor<'s> {
    text: &'s str,
    script: bool,
    scoping: &'s Scoping,
    intrinsics: &'s IntrinsicSymbols,
    limits: &'s JsAuthoringLimits,
    /// The function-like scopes around the node being read, innermost last.
    stack: Vec<Key>,
    /// Function declarations with the scope that owns them.
    declarations: Vec<(Key, Key)>,
    /// Whether this pass only declares bindings. The next one reads uses.
    declaring: bool,
    found: Survey,
}

/// What an expression is as a DOM origin.
enum OriginCall {
    Admitted { fresh: bool },
    BadArgument,
}

impl Surveyor<'_> {
    fn current(&self) -> Key {
        *self.stack.last().expect("the program is always a scope")
    }

    fn enter(&mut self, scope: Key) {
        if let Some(&parent) = self.stack.last() {
            self.found.parents.insert(scope, parent);
        }
        self.stack.push(scope);
    }

    fn leave(&mut self) {
        self.stack.pop();
    }

    /// Return the symbol an identifier's value reference resolves to.
    fn symbol(&self, identifier: &IdentifierReference<'_>) -> Option<SymbolId> {
        let reference = self.scoping.get_reference(identifier.reference_id.get()?);
        if !reference.is_value() {
            return None;
        }
        reference.symbol_id()
    }

    /// Return what `expression` is as an admitted origin call, if it is one.
    ///
    /// Only `document.querySelector()` and `document.createElement()` on the
    /// standard `document` are origins: a local, imported or declared
    /// `document` resolves to a symbol and is not the global.
    fn origin_call(&self, expression: &Expression<'_>) -> Option<OriginCall> {
        let Expression::CallExpression(call) = transparent(expression) else {
            return None;
        };
        self.origin_of(call)
    }

    fn origin_of(&self, call: &CallExpression<'_>) -> Option<OriginCall> {
        if call.optional {
            return None;
        }
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return None;
        };
        if member.optional
            || !matches!(
                member.property.name.as_str(),
                "querySelector" | "createElement"
            )
        {
            return None;
        }
        let Expression::Identifier(document) = &member.object else {
            return None;
        };
        if document.name != "document" {
            return None;
        }
        let reference = self.scoping.get_reference(document.reference_id.get()?);
        if reference.symbol_id().is_some() {
            return None;
        }
        Some(match call.arguments.as_slice() {
            [Argument::SpreadElement(_)] => OriginCall::BadArgument,
            [argument] => match argument.as_expression().and_then(static_string) {
                Some(_) => OriginCall::Admitted {
                    fresh: member.property.name == "createElement",
                },
                None => OriginCall::BadArgument,
            },
            _ => OriginCall::BadArgument,
        })
    }

    /// Record a new origin made in the current scope.
    fn new_origin(&mut self, fresh: bool) -> usize {
        self.found.origins.push(Origin {
            root: self.current(),
            fresh,
        });
        self.found.origins.len() - 1
    }

    /// Return whether `expression` is a direct `intent()` or `noIntent()`.
    fn is_explicit(&self, expression: &Expression<'_>) -> bool {
        let Expression::CallExpression(call) = transparent(expression) else {
            return false;
        };
        if call.optional {
            return false;
        }
        let Expression::Identifier(callee) = &call.callee else {
            return false;
        };
        self.symbol(callee)
            .and_then(|symbol| self.intrinsics.get(symbol))
            .is_some_and(|intrinsic| matches!(intrinsic, Intrinsic::Intent | Intrinsic::NoIntent))
    }

    /// Return whether `expression` is an `mf2` tagged template.
    fn is_descriptor(&self, expression: &Expression<'_>) -> bool {
        let Expression::TaggedTemplateExpression(tag) = transparent(expression) else {
            return false;
        };
        let Expression::Identifier(callee) = &tag.tag else {
            return false;
        };
        self.symbol(callee)
            .and_then(|symbol| self.intrinsics.get(symbol))
            .is_some_and(|intrinsic| intrinsic == Intrinsic::Mf2)
    }

    fn value(&self, expression: &Expression<'_>) -> Value {
        match transparent(expression) {
            Expression::StringLiteral(literal) => Value::Literal {
                span: literal.span,
                cooked: cook_string(self.text, literal, self.limits),
            },
            Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
                match template.quasis.first() {
                    Some(element) => Value::Literal {
                        span: template.span,
                        cooked: cook_template(self.text, element, self.limits),
                    },
                    None => Value::Dynamic(template.span),
                }
            }
            other if self.is_descriptor(other) => Value::Descriptor(expression.span()),
            _ => Value::Dynamic(expression.span()),
        }
    }

    fn receiver(&self, object: &Expression<'_>) -> Option<Receiver> {
        let object = transparent(object);
        if let Some(origin) = self.origin_call(object) {
            return Some(match origin {
                OriginCall::Admitted { .. } => Receiver::Direct {
                    root: self.current(),
                },
                OriginCall::BadArgument => Receiver::Unestablished(Unestablished::Argument),
            });
        }
        let Expression::Identifier(identifier) = object else {
            return None;
        };
        let symbol = self.symbol(identifier)?;
        Some(match *self.found.bindings.get(&symbol)? {
            Tracking::Tracked { origin, root, .. } if root == self.current() => {
                Receiver::Tracked { origin }
            }
            Tracking::Tracked { .. } => Receiver::Unestablished(Unestablished::Captured),
            Tracking::Unestablished(reason) => Receiver::Unestablished(reason),
            Tracking::AliasChain => Receiver::AliasChain,
        })
    }

    fn declare(&mut self, declarator: &VariableDeclarator<'_>) {
        let (BindingPattern::BindingIdentifier(binding), Some(init)) =
            (&declarator.id, &declarator.init)
        else {
            return;
        };
        let Some(symbol) = binding.symbol_id.get() else {
            return;
        };
        let followed = declarator.kind == VariableDeclarationKind::Const
            // A classic script's top-level bindings are shared with every
            // other script in the realm.
            && !(self.script && self.stack.len() == 1);
        let tracking = if let Some(origin) = self.origin_call(init) {
            match origin {
                OriginCall::BadArgument => Tracking::Unestablished(Unestablished::Argument),
                OriginCall::Admitted { fresh } if followed => Tracking::Tracked {
                    origin: self.new_origin(fresh),
                    root: self.current(),
                    depth: 0,
                },
                OriginCall::Admitted { .. } => Tracking::Unestablished(Unestablished::Binding),
            }
        } else if let Expression::Identifier(aliased) = transparent(init) {
            let Some(tracking) = self
                .symbol(aliased)
                .and_then(|s| self.found.bindings.get(&s))
            else {
                return;
            };
            match *tracking {
                Tracking::Tracked {
                    origin,
                    root,
                    depth,
                } if followed && root == self.current() => {
                    if depth + 1 > self.limits.alias_chain {
                        Tracking::AliasChain
                    } else {
                        Tracking::Tracked {
                            origin,
                            root,
                            depth: depth + 1,
                        }
                    }
                }
                Tracking::Tracked { .. } => Tracking::Unestablished(Unestablished::Binding),
                other => other,
            }
        } else {
            return;
        };
        self.found.bindings.insert(symbol, tracking);
    }

    fn assign(&mut self, assignment: &AssignmentExpression<'_>) {
        let AssignmentTarget::StaticMemberExpression(member) = &assignment.left else {
            return;
        };
        if member.property.name != "textContent" || self.is_explicit(&assignment.right) {
            return;
        }
        let compound = assignment.operator != AssignmentOperator::Assign;
        let Some(receiver) = self.receiver(&member.object) else {
            let literal = matches!(transparent(&assignment.right), Expression::StringLiteral(_))
                || matches!(
                    transparent(&assignment.right),
                    Expression::TemplateLiteral(template) if template.expressions.is_empty()
                );
            if literal && !compound {
                self.found.outside_profile += 1;
            }
            return;
        };
        self.found.candidates.push(Candidate {
            assignment: assignment.span,
            receiver,
            value: self.value(&assignment.right),
            compound,
        });
    }
}

impl<'a> Visit<'a> for Surveyor<'_> {
    fn visit_program(&mut self, program: &Program<'a>) {
        self.enter(key(program.span));
        walk::walk_program(self, program);
        self.leave();
    }

    fn visit_function(&mut self, function: &Function<'a>, flags: ScopeFlags) {
        let scope = key(function.span);
        if function.is_declaration() && !self.declaring {
            self.declarations.push((self.current(), scope));
        }
        self.enter(scope);
        walk::walk_function(self, function, flags);
        self.leave();
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'a>) {
        self.enter(key(arrow.span));
        walk::walk_arrow_function_expression(self, arrow);
        self.leave();
    }

    fn visit_class(&mut self, class: &Class<'a>) {
        self.enter(key(class.span));
        walk::walk_class(self, class);
        self.leave();
    }

    fn visit_ts_module_declaration(&mut self, module: &TSModuleDeclaration<'a>) {
        self.enter(key(module.span));
        walk::walk_ts_module_declaration(self, module);
        self.leave();
    }

    fn visit_with_statement(&mut self, statement: &WithStatement<'a>) {
        if !self.declaring {
            self.found.dynamic.insert(self.current());
        }
        walk::walk_with_statement(self, statement);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        // A direct `eval` in sloppy code can declare bindings in the scope
        // it runs in. Modules are always strict.
        if self.script && !self.declaring {
            if let Expression::Identifier(callee) = &call.callee {
                let unresolved = callee.reference_id.get().is_some_and(|reference| {
                    self.scoping.get_reference(reference).symbol_id().is_none()
                });
                if callee.name == "eval" && unresolved {
                    self.found.dynamic.insert(self.current());
                }
            }
        }
        walk::walk_call_expression(self, call);
    }

    fn visit_variable_declarator(&mut self, declarator: &VariableDeclarator<'a>) {
        if self.declaring {
            self.declare(declarator);
        }
        walk::walk_variable_declarator(self, declarator);
    }

    fn visit_export_named_declaration(&mut self, export: &ExportNamedDeclaration<'a>) {
        match &export.declaration {
            Some(Declaration::VariableDeclaration(declaration)) if !self.declaring => {
                for declarator in &declaration.declarations {
                    if let BindingPattern::BindingIdentifier(binding) = &declarator.id {
                        if let Some(symbol) = binding.symbol_id.get() {
                            self.found.exported.insert(symbol);
                        }
                    }
                }
            }
            _ => {}
        }
        walk::walk_export_named_declaration(self, export);
    }

    fn visit_assignment_expression(&mut self, assignment: &AssignmentExpression<'a>) {
        if !self.declaring {
            self.assign(assignment);
        }
        walk::walk_assignment_expression(self, assignment);
    }

    fn visit_identifier_reference(&mut self, identifier: &IdentifierReference<'a>) {
        if self.declaring {
            return;
        }
        let Some(Tracking::Tracked { origin, root, .. }) = self
            .symbol(identifier)
            .and_then(|symbol| self.found.bindings.get(&symbol))
            .copied()
        else {
            return;
        };
        let Some(at) = self.stack.iter().position(|scope| *scope == root) else {
            return;
        };
        // Every scope between the one that made the receiver and this use
        // captures it.
        for scope in self.stack[at + 1..].iter().copied() {
            self.found.captures.entry(scope).or_default().insert(origin);
        }
    }
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;

    use super::*;
    use crate::binding;
    use crate::grammar::Grammar;
    use crate::limits::tests::generous;
    use crate::test_support::{at, bindings, parse, reporter};

    /// Survey `text` and hand the result to `check`, with a way to find a
    /// binding's symbol by name and a scope's key by the text it starts with.
    fn surveyed(
        grammar: Grammar,
        text: &str,
        limits: &JsAuthoringLimits,
        check: impl FnOnce(&Survey, &dyn Fn(&str) -> SymbolId, &dyn Fn(&str) -> Key),
    ) {
        let allocator = Allocator::default();
        let parsed = parse(&allocator, text, grammar);
        let scoping = parsed.semantic.scoping();
        let intrinsics = binding::scan(parsed.program, &bindings(), &mut reporter(text)).unwrap();
        let survey = survey(
            parsed.program,
            text,
            grammar.is_script(),
            scoping,
            &intrinsics,
            limits,
        );
        let symbol = |name: &str| {
            scoping
                .symbol_ids()
                .find(|&symbol| scoping.symbol_name(symbol) == name)
                .unwrap_or_else(|| panic!("{name} is bound"))
        };
        let scope = |start: &str| {
            let (from, _) = at(text, start);
            let from = from as u32;
            *survey
                .parents
                .keys()
                .find(|(begin, _)| *begin == from)
                .unwrap_or_else(|| panic!("a scope starts at {start:?}"))
        };
        check(&survey, &symbol, &scope);
    }

    #[test]
    fn only_a_const_made_from_an_admitted_origin_or_aliasing_one_is_followed() {
        let text = "const a = document.querySelector('#a')\nconst b = a\n\
                    let c = document.createElement('c')\nconst d = document.querySelector(sel)\n\
                    const e = c\nexport function f() { const g = a }\n";
        surveyed(
            Grammar::JsModule,
            text,
            &generous(),
            |survey, symbol, scope| {
                let program = (0, text.len() as u32);
                assert!(matches!(
                    survey.bindings[&symbol("a")],
                    Tracking::Tracked { origin: 0, root, depth: 0 } if root == program
                ));
                assert!(matches!(
                    survey.bindings[&symbol("b")],
                    Tracking::Tracked {
                        origin: 0,
                        depth: 1,
                        ..
                    }
                ));
                assert!(matches!(
                    survey.bindings[&symbol("c")],
                    Tracking::Unestablished(Unestablished::Binding)
                ));
                assert!(matches!(
                    survey.bindings[&symbol("d")],
                    Tracking::Unestablished(Unestablished::Argument)
                ));
                // An alias keeps what it aliases.
                assert!(matches!(
                    survey.bindings[&symbol("e")],
                    Tracking::Unestablished(Unestablished::Binding)
                ));
                assert!(matches!(
                    survey.bindings[&symbol("g")],
                    Tracking::Unestablished(Unestablished::Binding)
                ));
                // Only a followed binding makes an origin to track.
                assert_eq!(survey.origins.len(), 1);
                let f = scope("function f()");
                assert_eq!(survey.captures[&f], BTreeSet::from([0]));
                assert_eq!(survey.parents[&f], program);
            },
        );
    }

    #[test]
    fn an_alias_past_the_bound_is_not_followed() {
        let text = "const a = document.querySelector('#a')\nconst b = a\nconst c = b\n";
        let mut limits = generous();
        limits.alias_chain = 1;
        surveyed(Grammar::JsModule, text, &limits, |survey, symbol, _| {
            assert!(matches!(
                survey.bindings[&symbol("b")],
                Tracking::Tracked { depth: 1, .. }
            ));
            assert!(matches!(
                survey.bindings[&symbol("c")],
                Tracking::AliasChain
            ));
        });
    }

    #[test]
    fn a_scripts_top_level_is_not_followed_but_its_functions_are() {
        let text = "const top = document.querySelector('#top')\n\
                    function f() { const inner = document.querySelector('#inner') }\n";
        surveyed(Grammar::JsScript, text, &generous(), |survey, symbol, _| {
            assert!(matches!(
                survey.bindings[&symbol("top")],
                Tracking::Unestablished(Unestablished::Binding)
            ));
            assert!(matches!(
                survey.bindings[&symbol("inner")],
                Tracking::Tracked { depth: 0, .. }
            ));
        });
    }

    #[test]
    fn with_and_sloppy_eval_make_a_scope_and_those_inside_it_dynamic() {
        let text = "function f(o) { with (o) { g() } function g() { h() } }\n\
                    function e(code) { eval(code) }\nfunction plain() {}\n";
        surveyed(Grammar::JsScript, text, &generous(), |survey, _, scope| {
            assert!(survey.is_dynamic(scope("function f(o)")));
            assert!(survey.is_dynamic(scope("function g()")));
            assert!(survey.is_dynamic(scope("function e(code)")));
            assert!(!survey.is_dynamic(scope("function plain()")));
        });
        // A module is strict: its `eval` declares nothing around it.
        let text = "function e(code) { eval(code) }\n";
        surveyed(Grammar::JsModule, text, &generous(), |survey, _, scope| {
            assert!(!survey.is_dynamic(scope("function e(code)")));
        });
    }

    #[test]
    fn a_function_declaration_captures_from_the_start_of_its_owner() {
        let text = "export function render() {\n  const pay = document.querySelector('#pay')\n  \
                    if (c) { function helper() { return pay } }\n  const arrow = () => pay\n}\n";
        surveyed(Grammar::JsModule, text, &generous(), |survey, _, scope| {
            let render = scope("function render()");
            assert_eq!(survey.hoisted[&render], BTreeSet::from([0]));
            // The arrow is made where it is written, so it is not hoisted.
            assert_eq!(survey.captures[&scope("() => pay")], BTreeSet::from([0]));
            assert_eq!(survey.hoisted.len(), 1);
        });
    }

    #[test]
    fn an_exported_binding_is_recorded() {
        let text = "export const pay = document.querySelector('#pay'), other = 1\nconst kept = 2\n";
        surveyed(Grammar::JsModule, text, &generous(), |survey, symbol, _| {
            assert_eq!(
                survey.exported,
                BTreeSet::from([symbol("pay"), symbol("other")])
            );
        });
    }

    #[test]
    fn each_text_content_assignment_says_what_its_receiver_and_value_are() {
        let text = "import { intent, mf2 } from 'fixture-authoring'\n\
                    const pay = document.querySelector('#pay')\n\
                    pay.textContent = 'Literal'\n\
                    pay.textContent = mf2`Tag`\n\
                    pay.textContent += label\n\
                    pay.textContent = intent('Explicit')\n\
                    document.createElement('p').textContent = `Direct`\n\
                    function later() { pay.textContent = 'Captured' }\n\
                    unknown.textContent = 'Outside'\n\
                    unknown.textContent = computed\n";
        surveyed(Grammar::JsModule, text, &generous(), |survey, _, _| {
            let summary: Vec<(&str, &str, bool)> = survey
                .candidates
                .iter()
                .map(|candidate| {
                    let receiver = match candidate.receiver {
                        Receiver::Direct { .. } => "direct",
                        Receiver::Tracked { .. } => "tracked",
                        Receiver::Unestablished(Unestablished::Captured) => "captured",
                        Receiver::Unestablished(_) => "unestablished",
                        Receiver::AliasChain => "alias-chain",
                    };
                    let value = match &candidate.value {
                        Value::Literal {
                            cooked: Ok(cooked), ..
                        } => match cooked.text.as_str() {
                            "Literal" => "Literal",
                            "Direct" => "Direct",
                            "Captured" => "Captured",
                            _ => "another literal",
                        },
                        Value::Literal { .. } => "unsupported",
                        Value::Descriptor(_) => "descriptor",
                        Value::Dynamic(_) => "dynamic",
                    };
                    (receiver, value, candidate.compound)
                })
                .collect();
            assert_eq!(
                summary,
                [
                    ("tracked", "Literal", false),
                    ("tracked", "descriptor", false),
                    ("tracked", "dynamic", true),
                    ("direct", "Direct", false),
                    ("captured", "Captured", false),
                ]
            );
            // The explicit form is left to its recognizer, and of the two
            // assignments with no known origin only the literal is counted.
            assert_eq!(survey.outside_profile, 1);
        });
    }
}
