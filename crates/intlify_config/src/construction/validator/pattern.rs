// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The regular expressions of Draft 7's `pattern` and `patternProperties`.
//!
//! Draft 7 takes its regular expressions from ECMA-262. Revision `"0"` reads a
//! pattern by ECMA-262's grammar with the `u` flag and no other flag: it
//! matches code points, case-sensitively; `.` matches any code point but a
//! line terminator; `^` and `$` hold only at the two ends of the input; and
//! `\w`, `\b`, and `\B` know only the ASCII word characters. A pattern is not
//! anchored: a string matches when any part of it does.
//!
//! Backreferences, lookarounds, named groups, modifiers, and Unicode property
//! escapes are valid ECMA-262 outside this subset, so a pattern that uses one
//! is refused as unsupported rather than as malformed. What remains describes
//! a regular language, and only whether a string matches is asked, never where
//! or what a group captured, so a compiled pattern runs as a nondeterministic
//! automaton over the code points. At each position of the input, the end
//! included, it enters each instruction at most once and tests each at most
//! once, whatever the pattern.
//!
//! Two capacities keep a short pattern from compiling to a large program:
//! groups nest at most [`MAX_NESTING`] deep, and the program, with every
//! counted repetition written out, holds at most [`MAX_INSTRUCTIONS`]
//! instructions. A pattern is checked in that order: its nesting, its syntax,
//! the constructs it uses, and its program's size.

use std::mem;

/// The deepest that groups may nest.
pub(crate) const MAX_NESTING: usize = 64;

/// The most instructions a compiled program may hold, its final match
/// included.
pub(crate) const MAX_INSTRUCTIONS: u64 = 1 << 16;

/// The largest Unicode code point.
const MAX_CODE_POINT: u32 = 0x10_FFFF;

/// The ASCII digits, `\d`.
const DIGITS: &[(u32, u32)] = &[(0x30, 0x39)];

/// The ASCII word characters, `\w`: digits, letters, and `_`.
const WORD: &[(u32, u32)] = &[(0x30, 0x39), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A)];

/// ECMA-262's white space and line terminators, `\s`: tab, line tabulation,
/// form feed, every space separator, and the zero-width no-break space, with
/// line feed, carriage return, line separator, and paragraph separator.
const SPACE: &[(u32, u32)] = &[
    (0x09, 0x0D),
    (0x20, 0x20),
    (0xA0, 0xA0),
    (0x1680, 0x1680),
    (0x2000, 0x200A),
    (0x2028, 0x2029),
    (0x202F, 0x202F),
    (0x205F, 0x205F),
    (0x3000, 0x3000),
    (0xFEFF, 0xFEFF),
];

/// ECMA-262's line terminators, which `.` does not match.
const LINE_TERMINATORS: &[(u32, u32)] = &[(0x0A, 0x0A), (0x0D, 0x0D), (0x2028, 0x2029)];

/// Why a pattern does not compile. The checks run in the order of the
/// variants, and the first that fails is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatternFailure {
    /// Groups nest deeper than [`MAX_NESTING`].
    Nesting,
    /// The source is not a pattern under ECMA-262's grammar with the `u` flag.
    Syntax,
    /// The source is valid ECMA-262 and uses a construct outside the subset.
    Unsupported(Construct),
    /// The compiled program would hold more than [`MAX_INSTRUCTIONS`].
    Size,
}

/// A valid ECMA-262 construct outside the subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Construct {
    /// `\1` and on, or `\k<name>`.
    Backreference,
    /// `(?=...)`, `(?!...)`, `(?<=...)`, or `(?<!...)`.
    Lookaround,
    /// `(?<name>...)`.
    NamedGroup,
    /// `(?i:...)` and the other groups that change a flag.
    Modifiers,
    /// `\p{...}` or `\P{...}`.
    PropertyEscape,
}

/// A compiled pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pattern {
    program: Vec<Instruction>,
    sets: Vec<Set>,
}

impl Pattern {
    /// Compile `source`, or report the first check it fails.
    pub(crate) fn compile(source: &str) -> Result<Self, PatternFailure> {
        let source: Vec<char> = source.chars().collect();
        if nesting(&source) > MAX_NESTING {
            return Err(PatternFailure::Nesting);
        }
        let mut parser = Parser::new(&source);
        let node = parser.disjunction()?;
        // A disjunction stops before the end only at a `)` that closes no
        // group.
        if parser.at < source.len() {
            return Err(PatternFailure::Syntax);
        }
        parser.resolve_references()?;
        if let Some(construct) = parser.unsupported {
            return Err(PatternFailure::Unsupported(construct));
        }
        if size(&node).saturating_add(1) > MAX_INSTRUCTIONS {
            return Err(PatternFailure::Size);
        }
        let mut compiler = Compiler::default();
        compiler.emit(&node);
        compiler.program.push(Instruction::Match);
        Ok(Self {
            program: compiler.program,
            sets: compiler.sets,
        })
    }

    /// Whether `haystack`, or any part of it, matches.
    pub(crate) fn matches(&self, haystack: &str) -> bool {
        self.run(haystack).0
    }

    /// The decision and the steps it took: each instruction entered and each
    /// instruction tested against a code point.
    fn run(&self, haystack: &str) -> (bool, u64) {
        let mut current = States::new(self.program.len());
        let mut next = States::new(self.program.len());
        let mut pending = Vec::new();
        let mut steps = 0;
        let mut code_points = haystack.chars().map(u32::from);
        let mut before = None;
        let mut at = code_points.next();
        loop {
            // A match may start at every position, which leaves the pattern
            // unanchored.
            if self.enter(&mut current, &mut pending, 0, (before, at), &mut steps) {
                return (true, steps);
            }
            let Some(code_point) = at else {
                return (false, steps);
            };
            let after = code_points.next();
            for index in 0..current.len {
                let state = current.dense[index];
                steps += 1;
                let consumed = match self.program[state] {
                    Instruction::Char(expected) => expected == code_point,
                    Instruction::Set(set) => self.sets[set].contains(code_point),
                    _ => false,
                };
                if consumed
                    && self.enter(
                        &mut next,
                        &mut pending,
                        state + 1,
                        (Some(code_point), after),
                        &mut steps,
                    )
                {
                    return (true, steps);
                }
            }
            mem::swap(&mut current, &mut next);
            next.clear();
            before = Some(code_point);
            at = after;
        }
    }

    /// Enter `state` and every instruction its empty moves reach at a position
    /// between the code points `around`; true when one of them is the match.
    fn enter(
        &self,
        states: &mut States,
        pending: &mut Vec<usize>,
        state: usize,
        around: (Option<u32>, Option<u32>),
        steps: &mut u64,
    ) -> bool {
        pending.push(state);
        while let Some(state) = pending.pop() {
            if !states.insert(state) {
                continue;
            }
            *steps += 1;
            match self.program[state] {
                Instruction::Split(first, second) => {
                    pending.push(second);
                    pending.push(first);
                }
                Instruction::Jump(target) => pending.push(target),
                Instruction::Assert(assertion) => {
                    if assertion.holds(around) {
                        pending.push(state + 1);
                    }
                }
                Instruction::Match => return true,
                Instruction::Char(_) | Instruction::Set(_) => {}
            }
        }
        false
    }
}

/// One instruction of a compiled program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Instruction {
    /// Consume this code point.
    Char(u32),
    /// Consume a code point of the set at this index.
    Set(usize),
    /// Go on only where the assertion holds.
    Assert(Assertion),
    /// Go on at both targets.
    Split(usize, usize),
    /// Go on at the target.
    Jump(usize),
    /// A match ends here.
    Match,
}

/// A zero-width assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Assertion {
    /// `^`: no code point before.
    Start,
    /// `$`: no code point after.
    End,
    /// `\b`: a word character on exactly one side.
    WordBoundary,
    /// `\B`: word characters on both sides or on neither.
    NotWordBoundary,
}

impl Assertion {
    fn holds(self, (before, after): (Option<u32>, Option<u32>)) -> bool {
        match self {
            Self::Start => before.is_none(),
            Self::End => after.is_none(),
            Self::WordBoundary => is_word(before) != is_word(after),
            Self::NotWordBoundary => is_word(before) == is_word(after),
        }
    }
}

fn is_word(code_point: Option<u32>) -> bool {
    code_point.is_some_and(|code_point| contains(WORD, code_point))
}

/// Code points, as sorted inclusive ranges that neither overlap nor touch.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Set(Vec<(u32, u32)>);

impl Set {
    fn new(mut ranges: Vec<(u32, u32)>) -> Self {
        ranges.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
        for (low, high) in ranges {
            match merged.last_mut() {
                Some(last) if low <= last.1.saturating_add(1) => last.1 = last.1.max(high),
                _ => merged.push((low, high)),
            }
        }
        Self(merged)
    }

    /// Every code point this set leaves out, surrogates included.
    fn complement(&self) -> Self {
        let mut ranges = Vec::with_capacity(self.0.len() + 1);
        let mut low = 0;
        for &(start, end) in &self.0 {
            if start > low {
                ranges.push((low, start - 1));
            }
            low = end + 1;
        }
        if low <= MAX_CODE_POINT {
            ranges.push((low, MAX_CODE_POINT));
        }
        Self(ranges)
    }

    fn contains(&self, code_point: u32) -> bool {
        contains(&self.0, code_point)
    }
}

fn contains(ranges: &[(u32, u32)], code_point: u32) -> bool {
    let index = ranges.partition_point(|&(_, high)| high < code_point);
    ranges.get(index).is_some_and(|&(low, _)| low <= code_point)
}

/// The set a class escape such as `\d` or `\W` stands for.
fn class_escape(letter: char) -> Set {
    let (ranges, negated) = match letter {
        'd' => (DIGITS, false),
        'D' => (DIGITS, true),
        's' => (SPACE, false),
        'S' => (SPACE, true),
        'w' => (WORD, false),
        _ => (WORD, true),
    };
    let set = Set(ranges.to_vec());
    if negated {
        set.complement()
    } else {
        set
    }
}

/// A parsed pattern.
#[derive(Debug)]
enum Node {
    /// Matches the empty string; also what a construct outside the subset
    /// parses to, since such a pattern never compiles.
    Empty,
    Char(u32),
    Set(Set),
    Assert(Assertion),
    Concat(Vec<Node>),
    /// Two or more alternatives.
    Alternation(Vec<Node>),
    /// From `min` to `max` repetitions, without an upper bound when `max` is
    /// absent. A count past `u64::MAX` saturates.
    Repeat {
        node: Box<Node>,
        min: u64,
        max: Option<u64>,
    },
}

/// The instructions `node` compiles to, saturating at `u64::MAX`.
fn size(node: &Node) -> u64 {
    match node {
        Node::Empty => 0,
        Node::Char(_) | Node::Set(_) | Node::Assert(_) => 1,
        Node::Concat(nodes) => nodes
            .iter()
            .fold(0, |total, node| total.saturating_add(size(node))),
        // A split before each alternative but the last, and a jump after it.
        Node::Alternation(nodes) => nodes
            .iter()
            .fold(2 * (nodes.len() as u64 - 1), |total, node| {
                total.saturating_add(size(node))
            }),
        Node::Repeat { node, min, max } => {
            let body = size(node);
            if body == 0 {
                return 0;
            }
            // An optional repetition is a split and the body; an unbounded one
            // is a split, the body, and a jump back to the split.
            let optional = max.map_or(body.saturating_add(2), |max| {
                max.saturating_sub(*min)
                    .saturating_mul(body.saturating_add(1))
            });
            min.saturating_mul(body).saturating_add(optional)
        }
    }
}

/// Writes a parsed pattern out as a program.
#[derive(Default)]
struct Compiler {
    program: Vec<Instruction>,
    sets: Vec<Set>,
}

impl Compiler {
    fn emit(&mut self, node: &Node) {
        match node {
            Node::Empty => {}
            Node::Char(code_point) => self.program.push(Instruction::Char(*code_point)),
            Node::Set(set) => {
                self.program.push(Instruction::Set(self.sets.len()));
                self.sets.push(set.clone());
            }
            Node::Assert(assertion) => self.program.push(Instruction::Assert(*assertion)),
            Node::Concat(nodes) => {
                for node in nodes {
                    self.emit(node);
                }
            }
            Node::Alternation(nodes) => {
                let mut jumps = Vec::with_capacity(nodes.len());
                for (index, node) in nodes.iter().enumerate() {
                    if index + 1 == nodes.len() {
                        self.emit(node);
                    } else {
                        let split = self.placeholder();
                        self.emit(node);
                        jumps.push(self.placeholder());
                        self.program[split] = Instruction::Split(split + 1, self.program.len());
                    }
                }
                let end = self.program.len();
                for jump in jumps {
                    self.program[jump] = Instruction::Jump(end);
                }
            }
            Node::Repeat { node, min, max } => {
                if size(node) == 0 {
                    return;
                }
                for _ in 0..*min {
                    self.emit(node);
                }
                if let Some(max) = max {
                    for _ in *min..*max {
                        let split = self.placeholder();
                        self.emit(node);
                        self.program[split] = Instruction::Split(split + 1, self.program.len());
                    }
                } else {
                    let split = self.placeholder();
                    self.emit(node);
                    self.program.push(Instruction::Jump(split));
                    self.program[split] = Instruction::Split(split + 1, self.program.len());
                }
            }
        }
    }

    /// Reserve an instruction to write once its targets are known.
    fn placeholder(&mut self) -> usize {
        self.program.push(Instruction::Match);
        self.program.len() - 1
    }
}

/// A set of instructions that empties in constant time.
struct States {
    dense: Vec<usize>,
    sparse: Vec<usize>,
    len: usize,
}

impl States {
    fn new(capacity: usize) -> Self {
        Self {
            dense: vec![0; capacity],
            sparse: vec![0; capacity],
            len: 0,
        }
    }

    /// Add `state`; false when it was in already.
    fn insert(&mut self, state: usize) -> bool {
        let index = self.sparse[state];
        if index < self.len && self.dense[index] == state {
            return false;
        }
        self.dense[self.len] = state;
        self.sparse[state] = self.len;
        self.len += 1;
        true
    }

    fn clear(&mut self) {
        self.len = 0;
    }
}

/// How deep the groups of `source` nest, read without parsing it: a backslash
/// hides the code point after it, and a parenthesis in a class is literal.
fn nesting(source: &[char]) -> usize {
    let mut depth = 0_usize;
    let mut deepest = 0;
    let mut class = false;
    let mut escaped = false;
    for &code_point in source {
        if escaped {
            escaped = false;
            continue;
        }
        match code_point {
            '\\' => escaped = true,
            '[' => class = true,
            ']' => class = false,
            '(' if !class => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            ')' if !class => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

type Parsed<T> = Result<T, PatternFailure>;

/// What a parenthesis opens.
enum Group {
    /// A group that matches its contents and may be quantified.
    Atom(Node),
    /// A lookaround, which the `u` flag does not let a quantifier follow.
    Lookaround,
}

/// One end of a class range.
enum ClassAtom {
    Char(u32),
    Set(Set),
}

/// Reads a pattern by ECMA-262's grammar with the `u` flag.
struct Parser<'a> {
    source: &'a [char],
    at: usize,
    /// The capturing groups, named ones included.
    groups: u64,
    names: Vec<String>,
    /// The largest number a backreference names, saturating.
    backreference: u64,
    named_references: Vec<String>,
    /// The first construct outside the subset, in source order.
    unsupported: Option<Construct>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a [char]) -> Self {
        Self {
            source,
            at: 0,
            groups: 0,
            names: Vec::new(),
            backreference: 0,
            named_references: Vec::new(),
            unsupported: None,
        }
    }

    fn peek(&self) -> Option<char> {
        self.source.get(self.at).copied()
    }

    fn peek_second(&self) -> Option<char> {
        self.source.get(self.at + 1).copied()
    }

    fn next(&mut self) -> Parsed<char> {
        let code_point = self.peek().ok_or(PatternFailure::Syntax)?;
        self.at += 1;
        Ok(code_point)
    }

    fn eat(&mut self, expected: char) -> bool {
        let found = self.peek() == Some(expected);
        if found {
            self.at += 1;
        }
        found
    }

    fn note(&mut self, construct: Construct) {
        self.unsupported.get_or_insert(construct);
    }

    /// `Disjunction`: alternatives separated by `|`.
    fn disjunction(&mut self) -> Parsed<Node> {
        let mut alternatives = vec![self.alternative()?];
        while self.eat('|') {
            alternatives.push(self.alternative()?);
        }
        Ok(if alternatives.len() == 1 {
            alternatives.swap_remove(0)
        } else {
            Node::Alternation(alternatives)
        })
    }

    /// `Alternative`: terms up to a `|`, a `)`, or the end.
    fn alternative(&mut self) -> Parsed<Node> {
        let mut terms = Vec::new();
        while !matches!(self.peek(), None | Some('|' | ')')) {
            terms.push(self.term()?);
        }
        Ok(match terms.len() {
            0 => Node::Empty,
            1 => terms.swap_remove(0),
            _ => Node::Concat(terms),
        })
    }

    /// `Term`: an assertion, or an atom with an optional quantifier.
    fn term(&mut self) -> Parsed<Node> {
        let atom = match self.next()? {
            '^' => return Ok(Node::Assert(Assertion::Start)),
            '$' => return Ok(Node::Assert(Assertion::End)),
            '\\' => match self.peek() {
                Some('b') => {
                    self.at += 1;
                    return Ok(Node::Assert(Assertion::WordBoundary));
                }
                Some('B') => {
                    self.at += 1;
                    return Ok(Node::Assert(Assertion::NotWordBoundary));
                }
                _ => self.atom_escape()?,
            },
            '(' => match self.group()? {
                Group::Atom(node) => node,
                Group::Lookaround => return Ok(Node::Empty),
            },
            '.' => Node::Set(Set(LINE_TERMINATORS.to_vec()).complement()),
            '[' => Node::Set(self.class()?),
            // A quantifier with nothing to repeat, and the brackets the `u`
            // flag never reads as literal code points.
            '*' | '+' | '?' | '{' | '}' | ']' => return Err(PatternFailure::Syntax),
            code_point => Node::Char(code_point.into()),
        };
        self.quantifier(atom)
    }

    /// An optional quantifier after `atom`. Laziness never changes whether a
    /// string matches, so a lazy quantifier reads as its greedy form.
    fn quantifier(&mut self, atom: Node) -> Parsed<Node> {
        let (min, max) = match self.peek() {
            Some('*') => {
                self.at += 1;
                (0, None)
            }
            Some('+') => {
                self.at += 1;
                (1, None)
            }
            Some('?') => {
                self.at += 1;
                (0, Some(1))
            }
            Some('{') => {
                self.at += 1;
                self.counted()?
            }
            _ => return Ok(atom),
        };
        self.eat('?');
        Ok(Node::Repeat {
            node: Box::new(atom),
            min,
            max,
        })
    }

    /// The bounds of `{n}`, `{n,}`, or `{n,m}` after the brace.
    fn counted(&mut self) -> Parsed<(u64, Option<u64>)> {
        let (min, min_digits) = self.decimal()?;
        if self.eat('}') {
            return Ok((min, Some(min)));
        }
        if !self.eat(',') {
            return Err(PatternFailure::Syntax);
        }
        if self.eat('}') {
            return Ok((min, None));
        }
        let (max, max_digits) = self.decimal()?;
        if !self.eat('}') || exceeds(min_digits, max_digits) {
            return Err(PatternFailure::Syntax);
        }
        Ok((min, Some(max)))
    }

    /// One or more decimal digits, as their value saturating at `u64::MAX`
    /// and as the digits themselves.
    fn decimal(&mut self) -> Parsed<(u64, &'a [char])> {
        let start = self.at;
        let mut value = 0_u64;
        while let Some(digit) = self.peek().and_then(|code_point| code_point.to_digit(10)) {
            self.at += 1;
            value = value.saturating_mul(10).saturating_add(digit.into());
        }
        if self.at == start {
            return Err(PatternFailure::Syntax);
        }
        Ok((value, &self.source[start..self.at]))
    }

    /// `AtomEscape` after its backslash, `\b` and `\B` aside.
    fn atom_escape(&mut self) -> Parsed<Node> {
        match self.next()? {
            letter @ ('d' | 'D' | 's' | 'S' | 'w' | 'W') => Ok(Node::Set(class_escape(letter))),
            'p' | 'P' => {
                self.property_escape()?;
                Ok(Node::Empty)
            }
            '1'..='9' => {
                self.at -= 1;
                let (number, _) = self.decimal()?;
                self.backreference = self.backreference.max(number);
                self.note(Construct::Backreference);
                Ok(Node::Empty)
            }
            'k' => {
                let name = self.group_name()?;
                self.named_references.push(name);
                self.note(Construct::Backreference);
                Ok(Node::Empty)
            }
            letter => self.character_escape(letter).map(Node::Char),
        }
    }

    /// `CharacterEscape` after its backslash and its first code point: the
    /// code point it stands for.
    fn character_escape(&mut self, letter: char) -> Parsed<u32> {
        match letter {
            'f' => Ok(0x0C),
            'n' => Ok(0x0A),
            'r' => Ok(0x0D),
            't' => Ok(0x09),
            'v' => Ok(0x0B),
            'c' => match self.next()? {
                control if control.is_ascii_alphabetic() => Ok(u32::from(control) % 32),
                _ => Err(PatternFailure::Syntax),
            },
            '0' if !self
                .peek()
                .is_some_and(|code_point| code_point.is_ascii_digit()) =>
            {
                Ok(0)
            }
            'x' => self.hex(2),
            'u' => self.unicode_escape(),
            // Under the `u` flag only a syntax character or `/` escapes itself.
            '^' | '$' | '\\' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
            | '/' => Ok(letter.into()),
            _ => Err(PatternFailure::Syntax),
        }
    }

    /// Exactly `digits` hexadecimal digits, as their value.
    fn hex(&mut self, digits: usize) -> Parsed<u32> {
        let mut value = 0;
        for _ in 0..digits {
            let digit = self.next()?.to_digit(16).ok_or(PatternFailure::Syntax)?;
            value = value * 16 + digit;
        }
        Ok(value)
    }

    /// The rest of a `\u` escape: `{`, a code point, and `}`, or four
    /// hexadecimal digits. The escape of a lead surrogate followed by the
    /// escape of a trail surrogate stands for the one code point they encode.
    fn unicode_escape(&mut self) -> Parsed<u32> {
        if self.eat('{') {
            let start = self.at;
            let mut value = 0_u32;
            while let Some(digit) = self.peek().and_then(|code_point| code_point.to_digit(16)) {
                self.at += 1;
                value = value.saturating_mul(16).saturating_add(digit);
            }
            if self.at == start || value > MAX_CODE_POINT || !self.eat('}') {
                return Err(PatternFailure::Syntax);
            }
            return Ok(value);
        }
        let lead = self.hex(4)?;
        if (0xD800..=0xDBFF).contains(&lead)
            && self.peek() == Some('\\')
            && self.peek_second() == Some('u')
        {
            let resume = self.at;
            self.at += 2;
            match self.hex(4) {
                Ok(trail) if (0xDC00..=0xDFFF).contains(&trail) => {
                    return Ok(0x1_0000 + ((lead - 0xD800) << 10) + (trail - 0xDC00));
                }
                _ => self.at = resume,
            }
        }
        Ok(lead)
    }

    /// The rest of `\p{...}` or `\P{...}` after the letter. The property is not
    /// looked up: the escape is outside the subset whatever it names.
    fn property_escape(&mut self) -> Parsed<()> {
        if !self.eat('{') {
            return Err(PatternFailure::Syntax);
        }
        let start = self.at;
        while self.peek().is_some_and(|code_point| {
            code_point.is_ascii_alphanumeric() || code_point == '_' || code_point == '='
        }) {
            self.at += 1;
        }
        if self.at == start || !self.eat('}') {
            return Err(PatternFailure::Syntax);
        }
        self.note(Construct::PropertyEscape);
        Ok(())
    }

    /// `<name>`. A name of ASCII code points must be an identifier; a name
    /// with another code point or an escape is not checked, since a name only
    /// appears in a construct outside the subset.
    fn group_name(&mut self) -> Parsed<String> {
        if !self.eat('<') {
            return Err(PatternFailure::Syntax);
        }
        let mut name = String::new();
        loop {
            match self.next()? {
                '>' => break,
                code_point => name.push(code_point),
            }
        }
        let checked = name
            .chars()
            .all(|code_point| code_point.is_ascii() && code_point != '\\');
        let mut code_points = name.chars();
        let identifier = code_points
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '$' || first == '_')
            && code_points.all(|rest| rest.is_ascii_alphanumeric() || rest == '$' || rest == '_');
        if checked && !identifier {
            return Err(PatternFailure::Syntax);
        }
        Ok(name)
    }

    /// What follows a `(`.
    fn group(&mut self) -> Parsed<Group> {
        if !self.eat('?') {
            self.groups = self.groups.saturating_add(1);
            return self.closed().map(Group::Atom);
        }
        match (self.peek(), self.peek_second()) {
            (Some(':'), _) => {
                self.at += 1;
                self.closed().map(Group::Atom)
            }
            (Some('=' | '!'), _) => {
                self.at += 1;
                self.lookaround()
            }
            (Some('<'), Some('=' | '!')) => {
                self.at += 2;
                self.lookaround()
            }
            (Some('<'), _) => {
                self.note(Construct::NamedGroup);
                let name = self.group_name()?;
                self.names.push(name);
                self.groups = self.groups.saturating_add(1);
                self.closed().map(Group::Atom)
            }
            _ => {
                self.modifiers()?;
                self.note(Construct::Modifiers);
                self.closed().map(Group::Atom)
            }
        }
    }

    fn lookaround(&mut self) -> Parsed<Group> {
        self.note(Construct::Lookaround);
        self.closed()?;
        Ok(Group::Lookaround)
    }

    /// A disjunction and the `)` that closes its group.
    fn closed(&mut self) -> Parsed<Node> {
        let node = self.disjunction()?;
        if self.eat(')') {
            Ok(node)
        } else {
            Err(PatternFailure::Syntax)
        }
    }

    /// The flags a modifier group adds and removes, up to its `:`. Neither
    /// side repeats a flag, the two sides share none, and they are not both
    /// empty.
    fn modifiers(&mut self) -> Parsed<()> {
        let added = self.flags()?;
        let removed = if self.eat('-') {
            Some(self.flags()?)
        } else {
            None
        };
        if !self.eat(':') {
            return Err(PatternFailure::Syntax);
        }
        if let Some(removed) = removed {
            if added | removed == 0 || added & removed != 0 {
                return Err(PatternFailure::Syntax);
            }
        }
        Ok(())
    }

    /// Modifier flags, one bit each for `i`, `m`, and `s`.
    fn flags(&mut self) -> Parsed<u8> {
        let mut flags = 0;
        loop {
            let flag = match self.peek() {
                Some('i') => 1,
                Some('m') => 2,
                Some('s') => 4,
                _ => return Ok(flags),
            };
            if flags & flag != 0 {
                return Err(PatternFailure::Syntax);
            }
            flags |= flag;
            self.at += 1;
        }
    }

    /// `CharacterClass` after its `[`.
    fn class(&mut self) -> Parsed<Set> {
        let negated = self.eat('^');
        let mut ranges = Vec::new();
        loop {
            let first = match self.next()? {
                ']' => break,
                code_point => self.class_atom(code_point)?,
            };
            if self.peek() == Some('-') && self.peek_second().is_some_and(|after| after != ']') {
                self.at += 1;
                let code_point = self.next()?;
                match (first, self.class_atom(code_point)?) {
                    (ClassAtom::Char(low), ClassAtom::Char(high)) if low <= high => {
                        ranges.push((low, high));
                    }
                    // A range out of order, or with a class escape at an end.
                    _ => return Err(PatternFailure::Syntax),
                }
            } else {
                match first {
                    ClassAtom::Char(code_point) => ranges.push((code_point, code_point)),
                    ClassAtom::Set(set) => ranges.extend(set.0),
                }
            }
        }
        let set = Set::new(ranges);
        Ok(if negated { set.complement() } else { set })
    }

    /// `ClassAtom`, from its first code point.
    fn class_atom(&mut self, code_point: char) -> Parsed<ClassAtom> {
        if code_point != '\\' {
            return Ok(ClassAtom::Char(code_point.into()));
        }
        match self.next()? {
            'b' => Ok(ClassAtom::Char(0x08)),
            '-' => Ok(ClassAtom::Char(0x2D)),
            letter @ ('d' | 'D' | 's' | 'S' | 'w' | 'W') => {
                Ok(ClassAtom::Set(class_escape(letter)))
            }
            'p' | 'P' => {
                self.property_escape()?;
                Ok(ClassAtom::Set(Set(Vec::new())))
            }
            letter => self.character_escape(letter).map(ClassAtom::Char),
        }
    }

    /// A backreference must name a group of the pattern, wherever the group
    /// is. Named groups are not compared with each other.
    fn resolve_references(&self) -> Parsed<()> {
        if self.backreference > self.groups
            || self
                .named_references
                .iter()
                .any(|name| !self.names.contains(name))
        {
            return Err(PatternFailure::Syntax);
        }
        Ok(())
    }
}

/// Whether the decimal `left` is larger than the decimal `right`, compared
/// exactly however many digits they have.
fn exceeds(left: &[char], right: &[char]) -> bool {
    fn significant(digits: &[char]) -> &[char] {
        let start = digits
            .iter()
            .position(|&digit| digit != '0')
            .unwrap_or(digits.len());
        &digits[start..]
    }
    let (left, right) = (significant(left), significant(right));
    left.len()
        .cmp(&right.len())
        .then_with(|| left.cmp(right))
        .is_gt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compiled(source: &str) -> Pattern {
        Pattern::compile(source)
            .unwrap_or_else(|failure| panic!("{source:?} fails with {failure:?}"))
    }

    fn matches(source: &str, haystack: &str) -> bool {
        compiled(source).matches(haystack)
    }

    fn character(code_point: u32) -> String {
        char::from_u32(code_point)
            .expect("a scalar value")
            .to_string()
    }

    #[test]
    fn a_pattern_matches_any_part_of_a_string() {
        assert!(matches("es", "expression"));
        assert!(!matches("^es", "expression"));
        assert!(matches("on$", "expression"));
        assert!(matches("", ""));
        assert!(matches("", "anything"));
        assert!(!matches("x", ""));
        assert!(matches("b", "abc"));
    }

    #[test]
    fn the_anchors_hold_only_at_the_two_ends_of_the_input() {
        assert!(matches("^a$", "a"));
        assert!(!matches("^a$", "\na"));
        assert!(!matches("^a$", "a\n"));
        assert!(!matches("a^", "a"));
        assert!(!matches("$a", "a"));
        assert!(matches("^$", ""));
        assert!(matches("(?:^|b)a", "ba"));
        assert!(!matches("(?:^|b)a", "ca"));
    }

    #[test]
    fn a_dot_matches_any_code_point_but_a_line_terminator() {
        let dot = compiled("^.$");
        for code_point in [0x61, 0x20, 0x85, 0xFEFF, 0x1_F600, MAX_CODE_POINT] {
            assert!(dot.matches(&character(code_point)), "{code_point:#x}");
        }
        for code_point in [0x0A, 0x0D, 0x2028, 0x2029] {
            assert!(!dot.matches(&character(code_point)), "{code_point:#x}");
        }
        assert!(!dot.matches(""));
        assert!(!dot.matches("ab"));
    }

    #[test]
    fn the_class_escapes_are_ecma_262s_ascii_and_white_space_sets() {
        let (space, other) = (compiled(r"^\s$"), compiled(r"^\S$"));
        for code_point in [
            0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x20, 0xA0, 0x1680, 0x2000, 0x2005, 0x200A, 0x2028,
            0x2029, 0x202F, 0x205F, 0x3000, 0xFEFF,
        ] {
            assert!(space.matches(&character(code_point)), "{code_point:#x}");
            assert!(!other.matches(&character(code_point)), "{code_point:#x}");
        }
        for code_point in [
            0x08, 0x0E, 0x1F, 0x21, 0x85, 0x9F, 0xA1, 0x167F, 0x1681, 0x180E, 0x1FFF, 0x200B,
            0x2027, 0x202A, 0x202E, 0x2030, 0x205E, 0x2060, 0x2FFF, 0x3001, 0xFEFE, 0xFF00,
        ] {
            assert!(!space.matches(&character(code_point)), "{code_point:#x}");
            assert!(other.matches(&character(code_point)), "{code_point:#x}");
        }
        let (digit, not_digit) = (compiled(r"^\d$"), compiled(r"^\D$"));
        for haystack in ["0", "5", "9"] {
            assert!(digit.matches(haystack) && !not_digit.matches(haystack));
        }
        for haystack in ["/", ":", "a", &character(0x0663), &character(0xFF10)] {
            assert!(!digit.matches(haystack) && not_digit.matches(haystack));
        }
        let (word, not_word) = (compiled(r"^\w$"), compiled(r"^\W$"));
        for haystack in ["a", "z", "A", "Z", "0", "9", "_"] {
            assert!(word.matches(haystack) && !not_word.matches(haystack));
        }
        for haystack in [
            "-",
            "@",
            "[",
            "`",
            "{",
            " ",
            &character(0xE9),
            &character(0x17F),
            &character(0x212A),
        ] {
            assert!(!word.matches(haystack) && not_word.matches(haystack));
        }
    }

    #[test]
    fn a_word_boundary_knows_only_ascii_word_characters() {
        assert!(matches(r"\ba", "a"));
        assert!(matches(r"a\b", "a-"));
        assert!(matches(r"a\b", &format!("a{}", character(0xE9))));
        assert!(!matches(r"a\b", "ab"));
        assert!(matches(r"a\B", "ab"));
        assert!(!matches(r"a\B", "a"));
        assert!(matches(r"\B", ""));
        assert!(!matches(r"\b", ""));
        assert!(!matches(r"^\b", &character(0xE9)));
        assert!(matches(r"-\B-", "--"));
    }

    #[test]
    fn a_code_point_is_one_atom_whether_written_or_escaped() {
        let face = character(0x1_F600);
        assert!(matches(r"^\u{1F600}$", &face));
        assert!(matches(r"^\uD83D\uDE00$", &face));
        assert!(matches(r"^[\u{1F600}-\u{1F64F}]$", &character(0x1_F610)));
        assert!(matches("^.$", &face));
        assert!(!matches("^..$", &face));
        // A lone surrogate's escape is valid and matches nothing a string
        // holds; a pair split across forms is two lone surrogates.
        assert!(!matches(r"\uD800", &face));
        assert!(!matches(r"\uD83D\u{DE00}", &face));
        assert!(matches(r"^[^\uD800]$", "a"));
        assert!(matches(r"\uD83Dx|y", "y"));
    }

    #[test]
    fn an_escape_stands_for_its_code_point() {
        assert!(matches(r"^\t\n\v\f\r$", "\t\n\x0B\x0C\r"));
        assert!(matches(r"^\cJ\ca\cZ$", "\n\x01\x1A"));
        assert!(matches(r"^\x41\u0042\u{43}\u{000044}$", "ABCD"));
        assert!(matches(r"^\0$", "\0"));
        assert!(matches(
            r"^\^\$\\\.\*\+\?\(\)\[\]\{\}\|\/$",
            r"^$\.*+?()[]{}|/"
        ));
        assert!(matches(r"^\u{10FFFF}$", &character(MAX_CODE_POINT)));
    }

    #[test]
    fn a_class_holds_code_points_ranges_and_class_escapes() {
        for (source, haystack, expected) in [
            ("[a-c]", "b", true),
            ("^[a-a]$", "a", true),
            ("[a-c]", "d", false),
            ("[^a-c]", "d", true),
            ("[^a-c]", "b", false),
            ("^[a-]$", "-", true),
            ("^[-a]$", "-", true),
            (r"^[\w-]$", "-", true),
            (r"^[\w-]$", "_", true),
            ("[]", "a", false),
            ("^[^]$", "\n", true),
            (r"^[\b]$", "\x08", true),
            (r"^[\-]$", "-", true),
            ("^[--/]$", ".", true),
            ("^[(]$", "(", true),
            ("^[[]$", "[", true),
            ("^[$^]$", "$", true),
            (r"^[\d\s]$", " ", true),
            (r"^[\d\s]$", "a", false),
            (r"^[^\W]$", "a", true),
            (r"^[^\W]$", "-", false),
            (r"^[\cJ-\r]$", "\x0B", true),
        ] {
            assert_eq!(
                matches(source, haystack),
                expected,
                "{source:?} on {haystack:?}"
            );
        }
    }

    #[test]
    fn a_quantifier_repeats_its_atom() {
        for (source, accepted, refused) in [
            ("^a{2,3}$", &["aa", "aaa"][..], &["a", "aaaa"][..]),
            ("^a{2}$", &["aa"], &["a", "aaa"]),
            ("^a{2,}$", &["aa", "aaaaa"], &["a"]),
            ("^a{0}$", &[""], &["a"]),
            ("^a{0001,1}$", &["a"], &["", "aa"]),
            ("^(?:ab)+$", &["ab", "abab"], &["", "aba"]),
            ("^a*?$", &["", "aaa"], &["b"]),
            ("^a+?b??$", &["a", "aab"], &["b"]),
            ("^a?$", &["", "a"], &["aa"]),
            ("^(?:a|)+$", &["", "aa"], &["b"]),
            ("^(?:)*$", &[""], &["a"]),
            ("^(a*)*$", &["", "aaa"], &["ab"]),
            ("^(?:a{2}){2,3}$", &["aaaa", "aaaaaa"], &["aa", "aaaaa"]),
        ] {
            for haystack in accepted {
                assert!(matches(source, haystack), "{source:?} on {haystack:?}");
            }
            for haystack in refused {
                assert!(!matches(source, haystack), "{source:?} on {haystack:?}");
            }
        }
    }

    #[test]
    fn alternatives_and_groups_match_as_written() {
        let pattern = compiled("^(?:a|bc|)$");
        for haystack in ["", "a", "bc"] {
            assert!(pattern.matches(haystack), "{haystack:?}");
        }
        for haystack in ["b", "abc", "c"] {
            assert!(!pattern.matches(haystack), "{haystack:?}");
        }
        assert!(matches("a|b", "xbx"));
        assert!(matches("^(a)(b)$", "ab"));
        assert!(matches("^(?:(a)|b)c$", "bc"));
        assert!(matches("|", "x"));
    }

    #[test]
    fn a_malformed_pattern_fails_as_syntax() {
        for source in [
            "(",
            ")",
            "a)",
            "[",
            "[a",
            "[a-",
            "a{",
            "a{1",
            "a{1,",
            "a{1,2",
            "a{,1}",
            "a{a}",
            "{",
            "}",
            "]",
            "*",
            "+",
            "?",
            "a**",
            "a{1}{2}",
            "a???",
            "a{2,1}",
            "a{99999999999999999999,99999999999999999998}",
            r"\",
            r"\a",
            r"\-",
            r"\c",
            r"\c1",
            r"\x4",
            r"\x4g",
            r"\u12",
            r"\u{}",
            r"\u{110000}",
            r"\u{FFFFFFFFFFFF}",
            r"\u{41",
            r"\01",
            "[b-a]",
            r"[\w-a]",
            r"[a-\d]",
            r"[\B]",
            r"[\1]",
            r"[\k<a>]",
            "^*",
            "$+",
            r"\b+",
            r"\B*",
            "(?",
            "(?x)",
            "(?-:a)",
            "(?ii:a)",
            "(?i-i:a)",
            "(?i",
            "(?<a",
            "(?<1a>x)",
            "(?<>x)",
            "(?<a-b>x)",
            r"\k",
            r"\ka",
            r"\k<a>",
            r"(?<b>x)\k<a>",
            r"\2(a)",
            r"\p",
            r"\p{}",
            r"\p{L",
            r"\p{L-}",
            "(?=a)*",
            "(?<=a)+",
        ] {
            assert_eq!(
                Pattern::compile(source),
                Err(PatternFailure::Syntax),
                "{source:?}"
            );
        }
    }

    #[test]
    fn valid_ecma_262_outside_the_subset_is_unsupported() {
        for (source, construct) in [
            ("(?=a)", Construct::Lookaround),
            ("(?!a)", Construct::Lookaround),
            ("(?<=a)", Construct::Lookaround),
            ("(?<!a)", Construct::Lookaround),
            (r"(a)\1", Construct::Backreference),
            (r"\1(a)", Construct::Backreference),
            (r"(a)(b)\2", Construct::Backreference),
            (r"\k<n>(?<n>a)", Construct::Backreference),
            ("(?<n>a)", Construct::NamedGroup),
            ("(?<$_9>a)", Construct::NamedGroup),
            ("(?<_a>x)", Construct::NamedGroup),
            ("(?<a$b>x)", Construct::NamedGroup),
            (r"(?<\u0061>x)", Construct::NamedGroup),
            (r"(?<n>a)\1", Construct::NamedGroup),
            (r"(?<n>a)\k<n>", Construct::NamedGroup),
            ("(?i:a)", Construct::Modifiers),
            ("(?-i:a)", Construct::Modifiers),
            ("(?ims-:a)", Construct::Modifiers),
            ("(?i-ms:a)", Construct::Modifiers),
            (r"\p{L}", Construct::PropertyEscape),
            (r"\P{Lu}", Construct::PropertyEscape),
            (r"[\p{Script=Greek}]", Construct::PropertyEscape),
            (r"\p{L}(?=a)", Construct::PropertyEscape),
        ] {
            assert_eq!(
                Pattern::compile(source),
                Err(PatternFailure::Unsupported(construct)),
                "{source:?}"
            );
        }
        // A name with a code point past ASCII is not checked.
        let name = format!("(?<{}>a)", character(0xE9));
        assert_eq!(
            Pattern::compile(&name),
            Err(PatternFailure::Unsupported(Construct::NamedGroup))
        );
    }

    #[test]
    fn the_first_check_to_fail_decides() {
        let deep = format!(
            "{}a{}",
            "(".repeat(MAX_NESTING + 1),
            ")".repeat(MAX_NESTING + 1)
        );
        assert_eq!(Pattern::compile(&deep), Err(PatternFailure::Nesting));
        assert_eq!(
            Pattern::compile(&format!("{deep}{{")),
            Err(PatternFailure::Nesting)
        );
        assert_eq!(Pattern::compile("(?=a)("), Err(PatternFailure::Syntax));
        assert_eq!(Pattern::compile(r"(a)\2"), Err(PatternFailure::Syntax));
        assert_eq!(
            Pattern::compile("(?=a)a{65536}"),
            Err(PatternFailure::Unsupported(Construct::Lookaround))
        );
    }

    #[test]
    fn groups_nest_at_most_sixty_four_deep() {
        let nested = |depth: usize| format!("{}a{}", "(".repeat(depth), ")".repeat(depth));
        assert!(compiled(&nested(MAX_NESTING)).matches("a"));
        assert_eq!(
            Pattern::compile(&nested(MAX_NESTING + 1)),
            Err(PatternFailure::Nesting)
        );
        // Neither an escaped parenthesis nor one in a class opens a group,
        // and groups side by side do not nest.
        assert!(Pattern::compile(&r"\(".repeat(MAX_NESTING + 1)).is_ok());
        assert!(Pattern::compile(&format!("[{}]", "(".repeat(MAX_NESTING + 1))).is_ok());
        assert!(Pattern::compile(&"(a)".repeat(MAX_NESTING + 1)).is_ok());
        // An escaped bracket opens no class, so the groups after it count.
        assert_eq!(
            Pattern::compile(&format!(r"\[{}", nested(MAX_NESTING + 1))),
            Err(PatternFailure::Nesting)
        );
        // A class ends at its bracket, so the groups after it count too.
        assert_eq!(
            Pattern::compile(&format!("[(]{}", nested(MAX_NESTING + 1))),
            Err(PatternFailure::Nesting)
        );
    }

    #[test]
    fn a_program_holds_at_most_its_capacity() {
        // Each `a` is one instruction, and the match is one more.
        assert_eq!(compiled("a{65535}").program.len(), 65_536);
        assert_eq!(Pattern::compile("a{65536}"), Err(PatternFailure::Size));
        assert_eq!(compiled("(?:a{255}){256}").program.len(), 255 * 256 + 1);
        assert_eq!(
            Pattern::compile("(?:a{256}){256}"),
            Err(PatternFailure::Size)
        );
        assert_eq!(
            Pattern::compile("a{18446744073709551616}"),
            Err(PatternFailure::Size)
        );
        assert_eq!(
            Pattern::compile("a{0,18446744073709551615}"),
            Err(PatternFailure::Size)
        );
        // A body that compiles to nothing repeats to nothing.
        assert_eq!(compiled("(?:){99999999999999999999}").program.len(), 1);
        assert_eq!(compiled("(?:){1,}").program.len(), 1);
    }

    #[test]
    fn a_size_is_the_length_of_the_program_it_compiles_to() {
        for text in [
            "",
            "a",
            "ab",
            "a|b",
            "a|b|",
            "|",
            "a*",
            "a+",
            "a?",
            "a{3}",
            "a{2,4}",
            "a{2,}",
            "(?:a|bc){2,3}",
            "(?:a*)*",
            "^$",
            r"\b\B",
            "[a-z]+.",
            "(?:){3}",
            "(?:)*",
            "(?:){0,2}",
            "(?:(?:)|a){2}",
            "((a){2}b?){0,2}c*",
        ] {
            let source: Vec<char> = text.chars().collect();
            let node = Parser::new(&source)
                .disjunction()
                .expect("the pattern parses");
            assert_eq!(
                size(&node) + 1,
                compiled(text).program.len() as u64,
                "{text:?}"
            );
        }
    }

    #[test]
    fn matching_takes_at_most_two_steps_per_instruction_at_each_position() {
        // Patterns a backtracking matcher takes exponential time on.
        for (source, haystack, expected) in [
            ("(a*)*b", "a".repeat(5_000), false),
            ("(a|a)*b", "a".repeat(5_000), false),
            ("(?:a?){30}a{30}", "a".repeat(30), true),
            ("(?:a|aa)+$", format!("{}b", "a".repeat(2_000)), false),
            (r"(\w+\s?)+$", format!("{}!", "word ".repeat(500)), false),
        ] {
            let pattern = compiled(source);
            let (matched, steps) = pattern.run(&haystack);
            assert_eq!(matched, expected, "{source:?}");
            let positions = haystack.chars().count() as u64 + 1;
            assert!(
                steps <= 2 * pattern.program.len() as u64 * positions,
                "{source:?} took {steps} steps"
            );
        }
    }

    #[test]
    fn the_steps_count_each_entry_and_each_test() {
        // `a` is a test of `a` and the match. At each position the automaton
        // enters the start, and it tests what it entered against the code
        // point there.
        assert_eq!(compiled("a").run(""), (false, 1));
        assert_eq!(compiled("a").run("b"), (false, 3));
        assert_eq!(compiled("a").run("a"), (true, 3));
        // `ab|c` enters the split and both alternatives, tests all three
        // against `c`, and enters the match.
        assert_eq!(compiled("ab|c").run("c"), (true, 7));
    }

    #[test]
    fn a_surrogate_pair_joins_only_a_lead_and_a_trail() {
        let joined = |source: &str| compiled(source).program.len() == 2;
        assert!(joined(r"\uD800\uDC00"));
        assert!(joined(r"\uDBFF\uDFFF"));
        assert!(matches(r"^\uD800\uDC00$", &character(0x1_0000)));
        assert!(matches(r"^\uDBFF\uDFFF$", &character(MAX_CODE_POINT)));
        // Outside the lead range, or with what is not a trail after it, each
        // escape stands for its own code point.
        for source in [
            r"\uD7FF\uDC00",
            r"\uDC00\uDC00",
            r"\uD800\uD800",
            r"\uD800\uDBFF",
            r"\uDBFF\uE000",
        ] {
            assert_eq!(compiled(source).program.len(), 3, "{source:?}");
        }
    }

    #[test]
    fn a_set_merges_its_ranges_and_complements_over_every_code_point() {
        let set = Set::new(vec![(5, 9), (0, 3), (4, 4), (20, 30), (25, 26)]);
        assert_eq!(set.0, vec![(0, 9), (20, 30)]);
        assert_eq!(set.complement().0, vec![(10, 19), (31, MAX_CODE_POINT)]);
        assert_eq!(Set(vec![(0, MAX_CODE_POINT)]).complement().0, vec![]);
        assert_eq!(Set(Vec::new()).complement().0, vec![(0, MAX_CODE_POINT)]);
        assert_eq!(
            Set(vec![(0, MAX_CODE_POINT - 1)]).complement().0,
            vec![(MAX_CODE_POINT, MAX_CODE_POINT)]
        );
        for code_point in [0, 9, 20, 30] {
            assert!(set.contains(code_point), "{code_point}");
        }
        for code_point in [10, 19, 31, MAX_CODE_POINT] {
            assert!(!set.contains(code_point), "{code_point}");
        }
    }

    #[test]
    fn a_decimal_is_compared_exactly() {
        let digits = |text: &str| text.chars().collect::<Vec<_>>();
        assert!(exceeds(&digits("10"), &digits("9")));
        assert!(!exceeds(&digits("9"), &digits("10")));
        assert!(!exceeds(&digits("0009"), &digits("9")));
        assert!(exceeds(&digits("0010"), &digits("009")));
        assert!(!exceeds(&digits("000"), &digits("0")));
        assert!(exceeds(
            &digits("99999999999999999999"),
            &digits("99999999999999999998")
        ));
    }
}
