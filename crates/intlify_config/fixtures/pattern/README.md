<!-- @license MIT -->

# Pattern fixtures

Draft 7 takes the regular expressions of `pattern` and `patternProperties` from ECMA-262, and design 015's [revision `"0"`](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#active-project-profile-specification-and-profile-resolution-specification-input) admits Draft 7 alone. Its validator reads a pattern by ECMA-262's grammar with the `u` flag and no other flag, and leaves out the constructs that a finite automaton cannot run in linear time or that this crate does not implement. This file states that subset and maps each of its rules to where it is checked.

## Running them

```sh
# every unit test
cargo test -p intlify_config --all-features --lib pattern

# the committed vectors, and the independent Node check of them
vp run vectors:pattern:check
```

## The subset

- **Syntax.** ECMA-262's pattern grammar with the `u` flag: a `{`, `}`, or `]` is never a literal, an escape is a syntax character, `/`, a class escape, or a character escape, and a class range has a single code point at each end.
- **Matching.** Code points are compared case-sensitively. `.` matches any code point but a line terminator (U+000A, U+000D, U+2028, U+2029). `^` and `$` hold only at the two ends of the input. `\d`, `\w`, `\b`, and `\B` know only ASCII digits and word characters, and `\s` is ECMA-262's white space and line terminators, every space separator included. A pattern is not anchored: a string matches when any part of it does. Whether a quantifier is lazy never changes that.
- **Outside the subset.** Backreferences (`\1`, `\k<name>`), lookarounds, named groups, modifier groups such as `(?i:...)`, and Unicode property escapes are valid ECMA-262 and are refused as unsupported.
- **Capacities.** Groups nest at most 64 deep, and a compiled program, with every counted repetition written out, holds at most 65,536 instructions.
- **Order.** A pattern is checked for its nesting, then its syntax, then the constructs it uses, then its program's size, and the first failure is reported. A backreference to a group the pattern does not have is a syntax error.

A compiled pattern runs as a nondeterministic automaton over the code points. At each position of the input, the end included, it enters and tests each instruction at most once, so its steps are bounded by twice the program's length times one more than the input's length.

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `vectors.json` | 39 patterns the subset compiles, each with the strings it matches and does not, 165 strings in all; 40 patterns decided on all 585 strings of up to three code points over an alphabet of eight, among them a line feed, a letter past ASCII, and an astral code point; and 78 refused patterns with their failure: two nest too deep, 58 are malformed, 15 use a construct outside the subset, and three compile to too large a program | `src/artifacts/vectors/pattern.rs` tests; `tools/shared-json-vectors` |

A decision is `1` when the pattern matches the string and `0` when it does not, in the order of the strings: the shorter first, and strings of one length in the alphabet's order, the first code point varying slowest.

The Node checker shares no code with this crate. It compiles every pattern with JavaScript's `RegExp` and the `u` flag, and decides every string with that engine. It requires each malformed pattern to throw a `SyntaxError` and every other refused pattern to compile, since each of those is valid ECMA-262, and it reads their nesting and constructs with its own scanner. ECMA-262 tries a match at each code point boundary and steps over a surrogate pair at once; V8 also tries the position between a pair's two surrogates, where `\B` holds, so the checker steps through the boundaries itself and matches at each one with the sticky flag.

## The matrix

The unit tests are at the end of `src/construction/validator/pattern.rs`, named here by their function.

| Rule | Checked by |
| --- | --- |
| A pattern is not anchored, and `^` and `$` hold only at the two ends of the input | `a_pattern_matches_any_part_of_a_string`, `the_anchors_hold_only_at_the_two_ends_of_the_input`. Node: `unanchored`, `anchors`, `anchors-in-alternatives`, and the exhaustive decisions |
| `.` matches any code point but a line terminator | `a_dot_matches_any_code_point_but_a_line_terminator`. Node: `dot` and the exhaustive decisions |
| `\d`, `\w`, and `\s` are ECMA-262's sets, and `\b` and `\B` know only ASCII word characters | `the_class_escapes_are_ecma_262s_ascii_and_white_space_sets`, `a_word_boundary_knows_only_ascii_word_characters`. Node: `digit`, `word`, `space`, `not-space`, `word-boundary`, `not-word-boundary`, `not-word-boundary-beside-astral` |
| A code point is one atom, whether written, escaped, or escaped as a surrogate pair; a lone surrogate matches nothing a string holds | `a_code_point_is_one_atom_whether_written_or_escaped`. Node: `astral-code-point`, `surrogate-pair-escape`, `lone-surrogate-escape` |
| An escape stands for its code point | `an_escape_stands_for_its_code_point`. Node: `control-escapes`, `control-letters`, `hexadecimal-escapes`, `nul`, `identity-escapes` |
| A class holds code points, ranges, and class escapes, and may be negated or empty | `a_class_holds_code_points_ranges_and_class_escapes`, `a_set_merges_its_ranges_and_complements_over_every_code_point`. Node: `class-ranges`, `negated-class`, `dash-after`, `dash-before`, `dash-after-escape`, `empty-class`, `any-class`, `class-escapes`, `class-literals` |
| A quantifier repeats its atom, lazy or not, and an alternative or a loop may match the empty string | `a_quantifier_repeats_its_atom`, `alternatives_and_groups_match_as_written`. Node: `counted`, `counted-exactly`, `counted-unbounded`, `lazy`, `nested-quantifiers`, `empty-loop`, `empty-alternative`, `groups`, `nested-loops` |
| A malformed pattern is a syntax error, and the bounds of `{n,m}` are compared exactly | `a_malformed_pattern_fails_as_syntax`, `a_decimal_is_compared_exactly`. Node: every pattern refused as `syntax` |
| A construct outside the subset is unsupported | `valid_ecma_262_outside_the_subset_is_unsupported`. Node: every pattern refused as `unsupported` |
| The checks run in order, and the first failure is reported | `the_first_check_to_fail_decides` |
| Groups nest at most 64 deep, read without parsing | `groups_nest_at_most_sixty_four_deep`. Node: every pattern refused as `nesting` |
| A program holds at most 65,536 instructions, and a pattern's size is the length of its program | `a_program_holds_at_most_its_capacity`, `a_size_is_the_length_of_the_program_it_compiles_to`. Node: every pattern refused as `size` |
| Matching takes at most two steps per instruction at each position | `matching_takes_at_most_two_steps_per_instruction_at_each_position` |
| The patterns Intlify's own schemas use compile and match | Node: `configuration-token`, `decimal-bound` |
| The committed vectors are the ones this crate writes | `src/artifacts/vectors/pattern.rs`: `the_committed_vectors_are_the_ones_this_crate_writes`, `the_exhaustive_strings_are_every_string_up_to_the_longest`, `every_chosen_pattern_has_strings_on_both_sides_or_says_why_not`, `every_failure_appears_among_the_refused_vectors`, `every_exhaustive_pattern_tells_some_strings_apart` |
