<!-- @license MIT -->

# Profile view vectors

Design 015's [LocalizationProjectProfile Semantic Model](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#localizationprojectprofile-semantic-model) defines the profile view: the closed, number-free JSON value of a checked profile's canonical semantic projection, with one member per semantic group. The profile semantic digest is design 017's [`H`](../../../../design/017-intlify-shared-artifact-and-version-admission-design.md#domain-separated-digest-function) over the view under `localization-project-profile-semantic-digest`, and two profiles are equal exactly when their views are. Design 015's [coverage inventory](../../../../design/015-intlify-project-profile-and-locale-policy-design.md#coverage-inventory) asks for frozen preimage and digest vectors of the profile view, computed by an independent implementation. This directory holds them.

The profiles here are test values. They are the resolver's complete test profile `web` and profiles that differ from it, resolved under the test Profile Specification, the finite test provider's locale data, whose dataset pin is a test string rather than a digest, the test vocabulary, and test artifacts. None of them is a profile a project declares.

## Running them

```sh
# every unit test, the reproduction of the vectors included
cargo test -p intlify_config --all-targets --all-features

# the vectors against the resolver, and the independent Node check
vp run vectors:profile-view:check

# print the vectors the resolver produces, to review before replacing the file
cargo test -p intlify_config --all-features --lib resolver::profile_view::tests::print_vectors_for_review -- --ignored --nocapture
```

## Data files

| File | Contents | Read by |
| --- | --- | --- |
| `vectors.json` | Sixteen profile views with their profile semantic digests, and twenty-one cases, each naming the view its test inputs project | `src/resolver/profile_view.rs` tests; `tools/shared-json-vectors` |

Each view appears once, under the name of the first case that projects it. The cases are:

| Case | Inputs | View |
| --- | --- | --- |
| `complete` | Profile `web`: requested `ja` and `en`, default `en`, source `en`, an alias, a fallback mapping, three coverage rules, every policy, two targets with Target Profiles, and one group whose server hydrates the browser | its own |
| `two-groups` | A second group, `jobs`, whose one target `worker` requests only `ja` and overrides the default with it | its own |
| `no-surface-classes` | A vocabulary without surface classes and no coverage rules: each requested locale has an empty decision row | its own |
| `project-id` | `projectId` `outlet` | its own |
| `selection-scope` | `selectionScope` `storefront.staging` | its own |
| `requested-locales` | `fr` requested as well, which gets its own decision row | its own |
| `default-requested-locale` | Default `ja`, which the browser also takes; the server's effective default follows | its own |
| `default-source-locale` | Source `ja` | its own |
| `no-default-source-locale` | No source locale: explicit absence, `null` | its own |
| `aliases` | The alias `de` maps to `ja` | its own |
| `message-fallback` | `ja` falls back to the Intent's source locale before `en` | its own |
| `default-mode` | Coverage default mode `fallback-allowed` | its own |
| `rules` | Without the first coverage rule, so only `checkout` may fall back in `ja` | its own |
| `provider-routing` | No provider-routing policy: `null` | its own |
| `target-default` | The browser does not override the default, so its effective default is the project's | its own |
| `hydration-relations` | The group has no hydration relation | its own |
| `another-profile-id` | The complete profile declared under the Profile ID `shop`, which the selector names | `complete` |
| `another-order` | Every set, object, and the artifact collection in another order | `complete` |
| `omitted-placement` | Delivery placement omitted, which is `duplicate` | `complete` |
| `omitted-default-mode` | Coverage default mode omitted, which is `direct-required` | `complete` |
| `redundant-rule` | Without the second coverage rule, on which no decision depends; only the Coverage Decision Basis changes | `complete` |

The digests cover values, not bytes, so the repository formatter may lay the file out.

## Who checks what

`resolver::profile_view`'s tests build every case's inputs, resolve and project them, and compare the committed file with the views and digests they produce. The same cases drive the unit tests of equality and inequality.

The Node checker shares no code with this crate and does not read the design document. It checks every view against its own copy of 015's closed shape:

- one member for each of the committed Profile Specification's `semanticGroups`, in that order, and no number anywhere
- every reference with exactly five members and the kind design 017 expects in its slot, and the semantic digests of the references, the Profile Specification, and the vocabulary in 017's `sha256:` spelling
- the bounds the Profile Specification's `resourceBounds` lists, each as its decimal string from 1 through 18446744073709551614
- every set in ascending UTF-8 byte order without repeats, and hydration relations ordered by server, then client
- a decision row for every requested locale, each with the same surface classes and a coverage mode in every cell
- each target's locales within the requested locales, and its effective default as its own default or else the project's
- every target in exactly one group, and each hydration relation between members of its group
- the placement mode `duplicate`, the only one revision `"0"` admits

It then frames and hashes every view with its own implementation of 017's `H`, compares the digest with the committed one, and checks that no two views and no two digests are equal, that every case names a view, and that some view is shared by several cases.

## The matrix

The unit tests are at the end of `src/resolver/profile_view.rs`, named here by their function.

| 015 requirement | Checked by |
| --- | --- |
| Frozen preimage and digest vectors of the profile view, computed by an independent implementation | `the_committed_vectors_are_the_resolvers`. Node: every digest recomputed from its view |
| One member per semantic group, in the Profile Specification's order | `a_checked_profile_projects_every_semantic_group_in_inventory_order`. Node: the committed Profile Specification's `semanticGroups` |
| The digest is 017's `H` under the domain 017 registers | `the_profile_semantic_digest_is_h_over_the_view`. Node: its own framing and hash |
| A difference outside the projection keeps the view and the digest | `declarations_of_one_profile_have_one_view_and_digest`. Vectors: the five cases that name `complete` |
| A semantic difference changes the view and the digest | `each_semantic_change_changes_the_view_and_its_digest`, `a_second_group_projects_its_own_target_and_relations`. Node: distinct views have distinct digests |
| A decision row for every requested locale, empty when the vocabulary has no surface class | `every_requested_locale_has_a_decision_row`. Node: the rows of every view |
| The view of a checked profile only when every semantic group is established | `resolver::profile_resolution`'s `the_view_exists_exactly_when_every_group_is_established` |

## Provisional shape

The file's fields and place are provisional. 015's Implementation Phase 6 closes the Conformance Suite: its Case Schema materializes the view's structure, a Resolver Case pairs its inputs with the expected view, and its Suite Index pins the vectors. Until then the inputs stay in the tests, and the vectors hold only the views, their digests, and the view each case projects.
