// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Whether a followed receiver's evidence holds on every path to a sink.
//!
//! One function is walked at a time, following the structure of its
//! statements and expressions rather than a prebuilt graph. The state is the
//! set of origins whose evidence some path has invalidated. It only grows:
//! nothing restores evidence, so a join is a union, a loop is walked until
//! its head stops growing, and a sink is proven only if its origin is valid
//! in every state the walk reaches it with.
//!
//! What counts as invalidating is decided by position, from an allow list. A
//! reference to a followed binding is harmless only as the object of a member
//! access, an operand of a comparison, `typeof`, `!` or `void`, a condition,
//! a discarded value, or the initializer of the `const` alias it names.
//! Anywhere else it exposes the receiver, so a syntax this walk does not
//! know falls on the side of invalidating. A reference from a nested function
//! is a capture, which takes effect where the function is made, or where the
//! enclosing function starts for a hoisted declaration.
//!
//! Where exactly an effect happens is allowed to be early. Invalidating at
//! the reference rather than at the call it is passed to can only make a
//! later sink less proven, never more.

use std::collections::{BTreeMap, BTreeSet};

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{
    ArrowFunctionExpression, AssignmentExpression, AssignmentTarget, BinaryExpression,
    BinaryOperator, BindingPattern, BreakStatement, Class, ComputedMemberExpression,
    ConditionalExpression, ContinueStatement, DoWhileStatement, Expression, ExpressionStatement,
    ForInStatement, ForOfStatement, ForStatement, ForStatementLeft, Function, IdentifierReference,
    IfStatement, LabeledStatement, PrivateFieldExpression, ReturnStatement, SequenceExpression,
    Statement, StaticMemberExpression, SwitchCase, SwitchStatement, TSModuleDeclaration,
    ThrowStatement, TryStatement, UnaryExpression, UnaryOperator, VariableDeclarator,
    WhileStatement,
};
use oxc_ast::AstKind;
use oxc_ast_visit::{walk, Visit};
use oxc_semantic::{ScopeFlags, Scoping, SymbolId};

use super::survey::{key, Key};
use crate::failure::ProducerFailure;
use crate::syntax::transparent;

/// How many steps are taken between two cancellation probes.
const PROBE_INTERVAL: u64 = 1024;

/// A set of origins, one bit each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Bits(Vec<u64>);

impl Bits {
    pub(super) fn new(width: usize) -> Self {
        Self(vec![0; width.div_ceil(64)])
    }

    pub(super) fn set(&mut self, bit: usize) {
        self.0[bit / 64] |= 1 << (bit % 64);
    }

    pub(super) fn contains(&self, bit: usize) -> bool {
        self.0[bit / 64] & (1 << (bit % 64)) != 0
    }

    fn union(&mut self, other: &Self) {
        for (mine, theirs) in self.0.iter_mut().zip(&other.0) {
            *mine |= *theirs;
        }
    }
}

/// The state at one point of the walk.
///
/// A point no path reaches, such as code after a `return`, is still walked
/// with the state it would have had, so what is written there is classified
/// the conservative way. It does not flow on to what follows.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Flow {
    invalid: Bits,
    live: bool,
}

impl Flow {
    fn dead(invalid: Bits) -> Self {
        Self {
            invalid,
            live: false,
        }
    }

    /// Merge two states, each reached on some path.
    fn join(self, other: Self) -> Self {
        match (self.live, other.live) {
            (true, false) => self,
            (false, true) => other,
            _ => {
                let mut invalid = self.invalid;
                invalid.union(&other.invalid);
                Self {
                    invalid,
                    live: self.live,
                }
            }
        }
    }
}

/// What one function's walk needs to know, with origins already as bits.
#[derive(Debug)]
pub(super) struct Root {
    pub(super) width: usize,
    /// The followed bindings made in this function.
    pub(super) symbols: BTreeMap<SymbolId, usize>,
    /// What each nested function-like scope captures.
    pub(super) captures: BTreeMap<Key, Vec<usize>>,
    /// What hoisted function declarations capture, from the function start.
    pub(super) hoisted: Vec<usize>,
    /// Followed bindings other modules can reach.
    pub(super) exported: BTreeSet<SymbolId>,
    /// The sinks whose evidence is asked for, by assignment.
    pub(super) sinks: BTreeMap<Key, usize>,
}

/// What one function's walk established.
#[derive(Debug, Default)]
pub(super) struct Verdict {
    /// Each sink reached, and whether its origin held every time.
    pub(super) valid: BTreeMap<Key, bool>,
    /// Whether the walk ran out of steps before it finished.
    pub(super) exhausted: bool,
}

/// Walk one function body.
pub(super) fn analyze<'a, C>(
    statements: &ArenaVec<'a, Statement<'a>>,
    root: &Root,
    scoping: &Scoping,
    steps: u64,
    cancelled: &C,
) -> Result<Verdict, ProducerFailure>
where
    C: Fn() -> bool + ?Sized,
{
    let mut walker = Walker {
        scoping,
        root,
        flow: Flow {
            invalid: Bits::new(root.width),
            live: true,
        },
        targets: Vec::new(),
        pending_labels: Vec::new(),
        finally: Vec::new(),
        accumulators: Vec::new(),
        harmless: BTreeSet::new(),
        forced: BTreeSet::new(),
        valid: BTreeMap::new(),
        steps: 0,
        limit: steps,
        exhausted: false,
        cancelled,
        failure: None,
    };
    for &bit in &root.hoisted {
        walker.invalidate(bit);
    }
    walker.visit_statements(statements);
    if let Some(failure) = walker.failure {
        return Err(failure);
    }
    Ok(Verdict {
        valid: walker.valid,
        exhausted: walker.exhausted,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Loop,
    Switch,
    Label,
}

/// Where a `break` or `continue` can go, and the states that went there.
#[derive(Debug)]
struct Target {
    kind: Kind,
    labels: Vec<String>,
    /// How many `finally` blocks were open when it was entered.
    finally_depth: usize,
    breaks: Option<Bits>,
    continues: Option<Bits>,
}

impl Target {
    fn arrived(jumps: Option<Bits>, otherwise: &Bits) -> Flow {
        match jumps {
            Some(invalid) => Flow {
                invalid,
                live: true,
            },
            None => Flow::dead(otherwise.clone()),
        }
    }
}

struct Walker<'w, C: ?Sized> {
    scoping: &'w Scoping,
    root: &'w Root,
    flow: Flow,
    targets: Vec<Target>,
    /// Labels waiting for the loop they name.
    pending_labels: Vec<String>,
    /// What each open `finally` block invalidates, innermost last.
    finally: Vec<Bits>,
    /// What each open `try` block has invalidated so far, innermost last.
    accumulators: Vec<Bits>,
    /// References in a position that does not expose them.
    harmless: BTreeSet<Key>,
    /// References that expose even in a harmless position, such as the
    /// object of a prototype write.
    forced: BTreeSet<Key>,
    valid: BTreeMap<Key, bool>,
    steps: u64,
    limit: u64,
    exhausted: bool,
    cancelled: &'w C,
    failure: Option<ProducerFailure>,
}

impl<C> Walker<'_, C>
where
    C: Fn() -> bool + ?Sized,
{
    fn stopped(&self) -> bool {
        self.exhausted || self.failure.is_some()
    }

    fn invalidate(&mut self, bit: usize) {
        self.flow.invalid.set(bit);
        for accumulated in &mut self.accumulators {
            accumulated.set(bit);
        }
    }

    fn capture(&mut self, scope: Key) {
        if let Some(bits) = self.root.captures.get(&scope) {
            for &bit in bits {
                self.invalidate(bit);
            }
        }
    }

    /// Mark the references an expression's value is made of as harmless.
    fn mark(&mut self, expression: &Expression<'_>) {
        let mut found = Vec::new();
        leaves(expression, &mut found);
        self.harmless.extend(found);
    }

    /// Mark the references an expression's value is made of as exposed.
    fn force(&mut self, expression: &Expression<'_>) {
        let mut found = Vec::new();
        leaves(expression, &mut found);
        self.forced.extend(found);
    }

    fn push_target(&mut self, kind: Kind, labels: Vec<String>) {
        self.targets.push(Target {
            kind,
            labels,
            finally_depth: self.finally.len(),
            breaks: None,
            continues: None,
        });
    }

    fn pop_target(&mut self) -> Target {
        self.targets.pop().expect("a target entered by this walk")
    }

    /// Send the current state to where a `break` or `continue` goes.
    ///
    /// Every `finally` between here and there runs on the way, so what each
    /// invalidates goes along.
    fn jump(&mut self, label: Option<&str>, continuing: bool) {
        let found = self.targets.iter().rposition(|target| match label {
            Some(label) => {
                target.labels.iter().any(|name| name == label)
                    && (!continuing || target.kind == Kind::Loop)
            }
            None if continuing => target.kind == Kind::Loop,
            None => matches!(target.kind, Kind::Loop | Kind::Switch),
        });
        if let Some(at) = found {
            let mut invalid = self.flow.invalid.clone();
            for runs in &self.finally[self.targets[at].finally_depth..] {
                invalid.union(runs);
            }
            let slot = if continuing {
                &mut self.targets[at].continues
            } else {
                &mut self.targets[at].breaks
            };
            match slot {
                Some(arrived) => arrived.union(&invalid),
                None => *slot = Some(invalid),
            }
        }
        self.flow.live = false;
    }

    fn take_labels(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_labels)
    }

    /// Walk a loop whose body the head repeats until the head stops growing.
    ///
    /// `iteration` walks one pass from the head and returns the state that
    /// goes back to it and the state leaving through the loop's own test.
    fn repeat(&mut self, labels: &[String], mut iteration: impl FnMut(&mut Self) -> (Flow, Flow)) {
        let entry = self.flow.clone();
        let mut head = entry.clone();
        loop {
            self.flow = head.clone();
            self.push_target(Kind::Loop, labels.to_vec());
            let (back, leaving) = iteration(self);
            let target = self.pop_target();
            let back = back.join(Target::arrived(target.continues, &head.invalid));
            let next = entry.clone().join(back);
            if next == head || self.stopped() {
                self.flow = leaving.join(Target::arrived(target.breaks, &head.invalid));
                return;
            }
            head = next;
        }
    }
}

/// Collect the references an expression's value can be, through wrappers
/// and value-preserving operators.
fn leaves(expression: &Expression<'_>, found: &mut Vec<Key>) {
    match transparent(expression) {
        Expression::Identifier(identifier) => found.push(key(identifier.span)),
        Expression::LogicalExpression(logical) => {
            leaves(&logical.left, found);
            leaves(&logical.right, found);
        }
        Expression::ConditionalExpression(conditional) => {
            leaves(&conditional.consequent, found);
            leaves(&conditional.alternate, found);
        }
        Expression::SequenceExpression(sequence) => {
            if let Some(last) = sequence.expressions.last() {
                leaves(last, found);
            }
        }
        _ => {}
    }
}

/// Return whether a statement is a loop, possibly behind more labels.
fn leads_to_loop(statement: &Statement<'_>) -> bool {
    match statement {
        Statement::LabeledStatement(labeled) => leads_to_loop(&labeled.body),
        other => other.is_iteration_statement(),
    }
}

impl<'a, C> Visit<'a> for Walker<'_, C>
where
    C: Fn() -> bool + ?Sized,
{
    fn enter_node(&mut self, _kind: AstKind<'a>) {
        self.steps += 1;
        if self.steps > self.limit {
            self.exhausted = true;
        }
        if self.steps.is_multiple_of(PROBE_INTERVAL) && self.failure.is_none() && (self.cancelled)()
        {
            self.failure = Some(ProducerFailure::Cancelled);
        }
    }

    fn visit_statement(&mut self, statement: &Statement<'a>) {
        if self.stopped() {
            return;
        }
        walk::walk_statement(self, statement);
    }

    fn visit_expression(&mut self, expression: &Expression<'a>) {
        if self.stopped() {
            return;
        }
        walk::walk_expression(self, expression);
    }

    fn visit_identifier_reference(&mut self, identifier: &IdentifierReference<'a>) {
        let Some(reference) = identifier.reference_id.get() else {
            return;
        };
        let reference = self.scoping.get_reference(reference);
        if !reference.is_value() {
            return;
        }
        let Some(&bit) = reference
            .symbol_id()
            .and_then(|symbol| self.root.symbols.get(&symbol))
        else {
            return;
        };
        let at = key(identifier.span);
        if self.forced.contains(&at) || !self.harmless.contains(&at) {
            self.invalidate(bit);
        }
    }

    fn visit_static_member_expression(&mut self, member: &StaticMemberExpression<'a>) {
        self.mark(&member.object);
        walk::walk_static_member_expression(self, member);
    }

    fn visit_computed_member_expression(&mut self, member: &ComputedMemberExpression<'a>) {
        self.mark(&member.object);
        walk::walk_computed_member_expression(self, member);
    }

    fn visit_private_field_expression(&mut self, member: &PrivateFieldExpression<'a>) {
        self.mark(&member.object);
        walk::walk_private_field_expression(self, member);
    }

    fn visit_unary_expression(&mut self, unary: &UnaryExpression<'a>) {
        match unary.operator {
            // Deleting the display property, or any computed one, changes
            // what an assignment to it does.
            UnaryOperator::Delete => match transparent(&unary.argument) {
                Expression::StaticMemberExpression(member)
                    if matches!(member.property.name.as_str(), "textContent" | "__proto__") =>
                {
                    self.force(&member.object);
                }
                Expression::ComputedMemberExpression(member) => self.force(&member.object),
                _ => {}
            },
            UnaryOperator::Typeof | UnaryOperator::LogicalNot | UnaryOperator::Void => {
                self.mark(&unary.argument);
            }
            _ => {}
        }
        walk::walk_unary_expression(self, unary);
    }

    fn visit_binary_expression(&mut self, binary: &BinaryExpression<'a>) {
        if matches!(
            binary.operator,
            BinaryOperator::Equality
                | BinaryOperator::Inequality
                | BinaryOperator::StrictEquality
                | BinaryOperator::StrictInequality
                | BinaryOperator::LessThan
                | BinaryOperator::LessEqualThan
                | BinaryOperator::GreaterThan
                | BinaryOperator::GreaterEqualThan
                | BinaryOperator::Instanceof
                | BinaryOperator::In
        ) {
            self.mark(&binary.left);
            self.mark(&binary.right);
        }
        walk::walk_binary_expression(self, binary);
    }

    fn visit_expression_statement(&mut self, statement: &ExpressionStatement<'a>) {
        self.mark(&statement.expression);
        walk::walk_expression_statement(self, statement);
    }

    fn visit_sequence_expression(&mut self, sequence: &SequenceExpression<'a>) {
        if let Some((_, discarded)) = sequence.expressions.split_last() {
            for expression in discarded {
                self.mark(expression);
            }
        }
        walk::walk_sequence_expression(self, sequence);
    }

    fn visit_variable_declarator(&mut self, declarator: &VariableDeclarator<'a>) {
        let bound = match &declarator.id {
            BindingPattern::BindingIdentifier(binding) => binding.symbol_id.get(),
            _ => None,
        };
        let followed = bound.and_then(|symbol| self.root.symbols.get(&symbol).copied());
        if followed.is_some() {
            if let Some(init) = &declarator.init {
                if matches!(transparent(init), Expression::Identifier(_)) {
                    self.mark(init);
                }
            }
        }
        walk::walk_variable_declarator(self, declarator);
        if let (Some(symbol), Some(bit)) = (bound, followed) {
            if self.root.exported.contains(&symbol) {
                self.invalidate(bit);
            }
        }
    }

    fn visit_assignment_expression(&mut self, assignment: &AssignmentExpression<'a>) {
        match &assignment.left {
            AssignmentTarget::StaticMemberExpression(member)
                if member.property.name == "__proto__" =>
            {
                self.force(&member.object);
            }
            AssignmentTarget::ComputedMemberExpression(member) => self.force(&member.object),
            _ => {}
        }
        walk::walk_assignment_expression(self, assignment);
        let at = key(assignment.span);
        if let Some(&bit) = self.root.sinks.get(&at) {
            let holds = !self.flow.invalid.contains(bit);
            self.valid
                .entry(at)
                .and_modify(|valid| *valid &= holds)
                .or_insert(holds);
        }
    }

    fn visit_conditional_expression(&mut self, conditional: &ConditionalExpression<'a>) {
        if self.stopped() {
            return;
        }
        self.mark(&conditional.test);
        self.visit_expression(&conditional.test);
        let tested = self.flow.clone();
        self.visit_expression(&conditional.consequent);
        let taken = std::mem::replace(&mut self.flow, tested);
        self.visit_expression(&conditional.alternate);
        let other = self.flow.clone();
        self.flow = taken.join(other);
    }

    fn visit_if_statement(&mut self, statement: &IfStatement<'a>) {
        if self.stopped() {
            return;
        }
        self.mark(&statement.test);
        self.visit_expression(&statement.test);
        let tested = self.flow.clone();
        self.visit_statement(&statement.consequent);
        let taken = std::mem::replace(&mut self.flow, tested);
        if let Some(alternate) = &statement.alternate {
            self.visit_statement(alternate);
        }
        let other = self.flow.clone();
        self.flow = taken.join(other);
    }

    fn visit_while_statement(&mut self, statement: &WhileStatement<'a>) {
        if self.stopped() {
            return;
        }
        let labels = self.take_labels();
        self.repeat(&labels, |walker| {
            walker.mark(&statement.test);
            walker.visit_expression(&statement.test);
            let tested = walker.flow.clone();
            walker.visit_statement(&statement.body);
            (walker.flow.clone(), tested)
        });
    }

    fn visit_do_while_statement(&mut self, statement: &DoWhileStatement<'a>) {
        if self.stopped() {
            return;
        }
        let labels = self.take_labels();
        self.repeat(&labels, |walker| {
            walker.visit_statement(&statement.body);
            // A `continue` goes to the test, so it joins the body here.
            let continued = walker
                .targets
                .last_mut()
                .and_then(|target| target.continues.take());
            if let Some(invalid) = continued {
                let arrived = Flow {
                    invalid,
                    live: true,
                };
                walker.flow = walker.flow.clone().join(arrived);
            }
            walker.mark(&statement.test);
            walker.visit_expression(&statement.test);
            (walker.flow.clone(), walker.flow.clone())
        });
    }

    fn visit_for_statement(&mut self, statement: &ForStatement<'a>) {
        if self.stopped() {
            return;
        }
        let labels = self.take_labels();
        if let Some(init) = &statement.init {
            self.visit_for_statement_init(init);
        }
        self.repeat(&labels, |walker| {
            if let Some(test) = &statement.test {
                walker.mark(test);
                walker.visit_expression(test);
            }
            let tested = walker.flow.clone();
            walker.visit_statement(&statement.body);
            // A `continue` goes to the update, so it joins the body here.
            let continued = walker
                .targets
                .last_mut()
                .and_then(|target| target.continues.take());
            if let Some(invalid) = continued {
                let arrived = Flow {
                    invalid,
                    live: true,
                };
                walker.flow = walker.flow.clone().join(arrived);
            }
            if let Some(update) = &statement.update {
                walker.mark(update);
                walker.visit_expression(update);
            }
            // Without a test the loop only leaves by jumping out.
            let leaving = if statement.test.is_some() {
                tested
            } else {
                Flow::dead(tested.invalid)
            };
            (walker.flow.clone(), leaving)
        });
    }

    fn visit_for_in_statement(&mut self, statement: &ForInStatement<'a>) {
        if self.stopped() {
            return;
        }
        self.iterate(&statement.left, &statement.right, &statement.body);
    }

    fn visit_for_of_statement(&mut self, statement: &ForOfStatement<'a>) {
        if self.stopped() {
            return;
        }
        self.iterate(&statement.left, &statement.right, &statement.body);
    }

    fn visit_switch_statement(&mut self, statement: &SwitchStatement<'a>) {
        if self.stopped() {
            return;
        }
        self.mark(&statement.discriminant);
        self.visit_expression(&statement.discriminant);
        // Tests run in order until one matches. Taking all of them first can
        // only put more in the state a case starts with.
        for case in &statement.cases {
            if let Some(test) = &case.test {
                self.mark(test);
                self.visit_expression(test);
            }
        }
        let tested = self.flow.clone();
        self.push_target(Kind::Switch, Vec::new());
        let mut falling = Flow::dead(tested.invalid.clone());
        for case in &statement.cases {
            self.flow = tested.clone().join(falling);
            self.visit_statements(&case.consequent);
            falling = self.flow.clone();
        }
        let target = self.pop_target();
        let mut leaving = falling.join(Target::arrived(target.breaks, &tested.invalid));
        if !statement.cases.iter().any(SwitchCase::is_default_case) {
            leaving = leaving.join(tested);
        }
        self.flow = leaving;
    }

    fn visit_labeled_statement(&mut self, statement: &LabeledStatement<'a>) {
        if self.stopped() {
            return;
        }
        let label = statement.label.name.to_string();
        if leads_to_loop(&statement.body) {
            self.pending_labels.push(label.clone());
        }
        self.push_target(Kind::Label, vec![label]);
        self.visit_statement(&statement.body);
        let target = self.pop_target();
        let current = self.flow.clone();
        self.flow = current.join(Target::arrived(target.breaks, &self.flow.invalid));
    }

    fn visit_break_statement(&mut self, statement: &BreakStatement<'a>) {
        if self.stopped() {
            return;
        }
        self.jump(
            statement.label.as_ref().map(|label| label.name.as_str()),
            false,
        );
    }

    fn visit_continue_statement(&mut self, statement: &ContinueStatement<'a>) {
        if self.stopped() {
            return;
        }
        self.jump(
            statement.label.as_ref().map(|label| label.name.as_str()),
            true,
        );
    }

    fn visit_return_statement(&mut self, statement: &ReturnStatement<'a>) {
        if self.stopped() {
            return;
        }
        if let Some(argument) = &statement.argument {
            self.visit_expression(argument);
        }
        self.flow.live = false;
    }

    fn visit_throw_statement(&mut self, statement: &ThrowStatement<'a>) {
        if self.stopped() {
            return;
        }
        self.visit_expression(&statement.argument);
        self.flow.live = false;
    }

    fn visit_try_statement(&mut self, statement: &TryStatement<'a>) {
        if self.stopped() {
            return;
        }
        let width = self.root.width;
        let entry = self.flow.clone();
        // What the `finally` block invalidates does not depend on the state
        // it starts with, so it is found first and carried by every jump
        // that leaves through it.
        let finalizes = statement.finalizer.as_ref().map(|finalizer| {
            self.accumulators.push(Bits::new(width));
            self.visit_block_statement(finalizer);
            self.flow = entry.clone();
            self.accumulators
                .pop()
                .expect("the accumulator just pushed")
        });
        if let Some(runs) = &finalizes {
            self.finally.push(runs.clone());
        }

        // Any point of the block can throw, so the handler starts with
        // everything the block invalidated anywhere.
        self.accumulators.push(Bits::new(width));
        self.visit_block_statement(&statement.block);
        let in_block = self
            .accumulators
            .pop()
            .expect("the accumulator just pushed");
        let after_block = self.flow.clone();

        let mut in_handler = Bits::new(width);
        let after_handler = statement.handler.as_ref().map(|handler| {
            let mut invalid = entry.invalid.clone();
            invalid.union(&in_block);
            self.flow = Flow {
                invalid,
                live: true,
            };
            self.accumulators.push(Bits::new(width));
            self.visit_catch_clause(handler);
            in_handler = self
                .accumulators
                .pop()
                .expect("the accumulator just pushed");
            self.flow.clone()
        });
        if finalizes.is_some() {
            self.finally.pop();
        }

        match &statement.finalizer {
            Some(finalizer) => {
                // The block runs after a normal end, a jump, or a throw, so
                // it starts with all of them.
                let completes =
                    after_block.live || after_handler.as_ref().is_some_and(|flow| flow.live);
                let mut invalid = entry.invalid.clone();
                invalid.union(&in_block);
                invalid.union(&in_handler);
                invalid.union(&after_block.invalid);
                if let Some(handled) = &after_handler {
                    invalid.union(&handled.invalid);
                }
                self.flow = Flow {
                    invalid,
                    live: true,
                };
                self.visit_block_statement(finalizer);
                self.flow.live = self.flow.live && completes;
            }
            None => {
                self.flow = match after_handler {
                    Some(handled) => after_block.join(handled),
                    None => after_block,
                };
            }
        }
    }

    fn visit_function(&mut self, function: &Function<'a>, _flags: ScopeFlags) {
        if self.stopped() {
            return;
        }
        // A declaration took effect where this function started.
        if !function.is_declaration() {
            self.capture(key(function.span));
        }
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'a>) {
        if self.stopped() {
            return;
        }
        self.capture(key(arrow.span));
    }

    fn visit_class(&mut self, class: &Class<'a>) {
        if self.stopped() {
            return;
        }
        self.capture(key(class.span));
    }

    fn visit_ts_module_declaration(&mut self, module: &TSModuleDeclaration<'a>) {
        if self.stopped() {
            return;
        }
        self.capture(key(module.span));
    }
}

impl<C> Walker<'_, C>
where
    C: Fn() -> bool + ?Sized,
{
    /// Walk a `for`-`in` or `for`-`of` loop.
    fn iterate<'a>(
        &mut self,
        left: &ForStatementLeft<'a>,
        right: &Expression<'a>,
        body: &Statement<'a>,
    ) {
        let labels = self.take_labels();
        self.visit_expression(right);
        self.repeat(&labels, |walker| {
            // The loop may run no more times from the head.
            let head = walker.flow.clone();
            walker.visit_for_statement_left(left);
            walker.visit_statement(body);
            (walker.flow.clone(), head)
        });
    }
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;

    use super::*;
    use crate::grammar::Grammar;
    use crate::test_support::{at, parse};

    fn bits(width: usize, set: &[usize]) -> Bits {
        let mut bits = Bits::new(width);
        for &bit in set {
            bits.set(bit);
        }
        bits
    }

    fn root(width: usize) -> Root {
        Root {
            width,
            symbols: BTreeMap::new(),
            captures: BTreeMap::new(),
            hoisted: Vec::new(),
            exported: BTreeSet::new(),
            sinks: BTreeMap::new(),
        }
    }

    #[test]
    fn bits_are_a_set_across_words() {
        let mut first = bits(130, &[0, 64, 129]);
        assert!(first.contains(0) && first.contains(64) && first.contains(129));
        assert!(!first.contains(1) && !first.contains(128));
        first.union(&bits(130, &[1, 128]));
        assert_eq!(first, bits(130, &[0, 1, 64, 128, 129]));
        assert_eq!(Bits::new(0), Bits(Vec::new()));
    }

    #[test]
    fn a_join_keeps_only_live_paths_and_merges_them() {
        let live = |set: &[usize]| Flow {
            invalid: bits(8, set),
            live: true,
        };
        let dead = |set: &[usize]| Flow::dead(bits(8, set));
        assert_eq!(live(&[1]).join(live(&[2])), live(&[1, 2]));
        // A path nothing reaches adds nothing to one that is reached.
        assert_eq!(live(&[1]).join(dead(&[2])), live(&[1]));
        assert_eq!(dead(&[2]).join(live(&[1])), live(&[1]));
        // Two unreached paths keep everything, for code read after them.
        assert_eq!(dead(&[1]).join(dead(&[2])), dead(&[1, 2]));
    }

    #[test]
    fn a_value_is_made_of_the_references_it_can_evaluate_to() {
        let text = "(a || b) ? (c, d) : (e as T)!";
        let allocator = Allocator::default();
        let expression = oxc_parser::Parser::new(&allocator, text, Grammar::TsModule.source_type())
            .parse_expression()
            .unwrap();
        let mut found = Vec::new();
        leaves(&expression, &mut found);
        let spans: Vec<&str> = found
            .iter()
            .map(|(start, end)| &text[*start as usize..*end as usize])
            .collect();
        // The test is a condition, not the value, and a sequence is its
        // last expression.
        assert_eq!(spans, ["d", "e"]);
    }

    #[test]
    fn a_label_leads_to_a_loop_through_more_labels_only() {
        let text = "a: b: while (c) {}\nd: { }\n";
        let allocator = Allocator::default();
        let parsed = parse(&allocator, text, Grammar::JsModule);
        let [first, second] = &parsed.program.body[..] else {
            panic!("two statements");
        };
        assert!(leads_to_loop(first));
        assert!(!leads_to_loop(second));
    }

    #[test]
    fn proof_steps_count_entered_nodes_and_bound_them_inclusively() {
        // Each empty statement is one entered node and nothing else.
        let text = ";;;;;\n";
        let allocator = Allocator::default();
        let parsed = parse(&allocator, text, Grammar::JsModule);
        let scoping = parsed.semantic.scoping();
        let run = |steps| {
            analyze(&parsed.program.body, &root(0), scoping, steps, &|| false)
                .unwrap()
                .exhausted
        };
        assert!(!run(5));
        assert!(run(4));
    }

    #[test]
    fn a_sink_holds_only_if_its_origin_held_every_time_it_was_reached() {
        let text = "const pay = document.querySelector('#pay')\n\
                    while (c) { pay.textContent = 'Pay'; customize(pay) }\n";
        let allocator = Allocator::default();
        let parsed = parse(&allocator, text, Grammar::JsModule);
        let scoping = parsed.semantic.scoping();
        let pay = scoping
            .symbol_ids()
            .find(|&symbol| scoping.symbol_name(symbol) == "pay")
            .unwrap();
        let sink = at(text, "pay.textContent = 'Pay'");
        let mut walk = root(1);
        walk.symbols.insert(pay, 0);
        walk.sinks.insert((sink.0 as u32, sink.1 as u32), 0);
        let verdict = analyze(&parsed.program.body, &walk, scoping, u64::MAX, &|| false).unwrap();
        // The first pass reaches it valid, the next one after the call.
        assert_eq!(
            verdict.valid.get(&(sink.0 as u32, sink.1 as u32)),
            Some(&false)
        );
        assert!(!verdict.exhausted);
    }

    #[test]
    fn a_probe_asking_to_stop_stops_the_walk() {
        let mut text = String::new();
        for _ in 0..2048 {
            text.push(';');
        }
        let allocator = Allocator::default();
        let parsed = parse(&allocator, &text, Grammar::JsModule);
        let result = analyze(
            &parsed.program.body,
            &root(0),
            parsed.semantic.scoping(),
            u64::MAX,
            &|| true,
        );
        assert_eq!(
            result.map(|verdict| verdict.exhausted),
            Err(ProducerFailure::Cancelled)
        );
    }
}
