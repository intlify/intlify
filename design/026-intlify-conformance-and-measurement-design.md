# Intlify Conformance and Measurement Design

## Purpose

This design defines how Intlify proves two different properties without confusing them:

1. a Producer, compiler component, Target Exporter, Localization Execution Layer, or physical engine preserves the applicable Intlify semantics; and
2. one physical implementation has an observable artifact-size, initialization, loading, formatting, toolchain, or memory cost under a recorded workload and environment.

The first property is **conformance**. It is a semantic pass, fail, or non-applicable result over versioned fixtures and claimed capabilities. The second property is **measurement**. It is physical evidence that may vary with hardware, operating system, build configuration, runtime, allocator, power state, and sampling policy.

The design also defines the shared performance implementation architecture needed before those measurements are useful: remove duplicate and optional work, align allocation with data lifetime, keep common paths narrow, bound parallel and cached work, measure host boundaries and generated-output delivery, and reserve low-level specialization for demonstrated bottlenecks. An opt-in Profiler locates cost; an uninstrumented Benchmark quantifies it; an admitted comparison and budget retain the improvement.

026 provides the common verification model between component-owned specifications and product-wide reporting. It does not take ownership of a component's semantic operation names. For example, 015 continues to own `profile_locale_resolve / locale_canonicalization` and its exact interval. 026 defines how that owner-produced observation records exact units, samples, environment, and workload identity, and when it may be compared with a baseline or evaluated against a budget.

![Intlify conformance and measurement architecture](./assets/026-intlify-conformance-and-measurement-architecture.svg)

The diagram shows the verification-evidence path. The Performance Implementation Architecture and opt-in Profiling Specification are cross-cutting inputs to owner harnesses; they do not add another semantic authority or evidence-admission layer.

In practical terms, the design establishes the following flow:

```text
component or target specification
  -> owner-defined fixtures, phase/cost names, and semantic checksums
  -> owner-produced conformance or benchmark result
  -> 026 admission and lossless common projection
  -> immutable conformance and measurement evidence
  -> explicitly compatible comparison and budget evaluation, or descriptive report projection
  -> structured report, CI decision, or Release evidence
```

The common projection is intentionally not one universal benchmark runner. Existing and future parser, formatter, resource, linker, profile-resolver, Runtime, browser, mobile, and native harnesses retain their natural execution mechanisms and product-owned checked result schemas. A conforming projection preserves their owner, schema revision, fixture identity, phase, cost, interval, workload, and semantic observation instead of renaming or averaging them into one score.

This separation lets the 015 minimum implementation record projection-ready measurements from its first implementation phase. Numeric performance gates can be admitted later without changing the resolver's semantic APIs, measurement points, or result meaning.

## Goals

- Define one language-neutral model for conformance evidence, physical measurement evidence, comparisons, budgets, and their outcomes.
- Preserve each owning design's exact fixture, phase, cost, interval, checksum, and result-schema authority.
- Make semantic conformance independent from machine-dependent performance.
- Give all duration, size, memory, count, difference, and ratio values exact cross-language representations.
- Record enough workload, build, runtime, and environment information to decide comparability rather than assuming it.
- Define common artifact-size, delivery-topology, initialization, loading, formatting, host-boundary, toolchain, and memory categories while retaining finer owner-specific operations.
- Make process reuse, engine reuse, preparation state, cache state, runtime-compilation state, managed-heap state, scratch reuse, output-buffer reuse, text, structured-parts, and optional-output paths explicit.
- Define immutable baseline selection and reproducible regression and budget evaluation.
- Allow deterministic size budgets to gate ordinary CI while limiting duration and memory gates to admitted, sufficiently stable runner classes.
- Define capability evidence that proves every claimed required capability through applicable conformance cases.
- Define common Finding-projection verification without making presentation text the semantic source of truth.
- Define exact logical-render equivalence for hydration-coupled Browser and SSR targets and controlled equivalence for other cross-target execution paths.
- Keep result collection useful for local development, continuous integration, release admission, and longitudinal reporting.
- Prevent benchmark instrumentation, timestamps, or machine identity from changing product semantics, artifact identity, or localization output.
- Define cross-component performance implementation principles for avoiding duplicate work, unnecessary materialization, and avoidable allocation before applying low-level optimization.
- Treat core execution, host-language boundaries, generated-output topology, and complete product workflows as distinct performance surfaces.
- Define an opt-in profiling model that can diagnose hierarchical time and allocation cost without imposing runtime work on ordinary builds.
- Separate profiling for cause discovery, benchmarking for quantified verification, and regression gates for retained performance.
- Give 015 an initial observational profile that avoids reworking its benchmark record path after numeric policy is introduced.

## Non-Goals

- Defining the semantic behavior owned by 015, 017, 019, 020, 023, 024, or 025.
- Replacing owner-defined conformance suites or product benchmark result schemas with one universal runner or schema.
- Defining one mandatory physical MF2 engine, locale-service implementation, timer, allocator, profiler, CI vendor, or benchmark framework.
- Freezing component-specific Workspace types, arena choices, collection implementations, cache layouts, prepared-message layouts, or output-buffer APIs.
- Treating profiler output as benchmark evidence or allowing an instrumented diagnostic build to establish an uninstrumented performance budget automatically.
- Making results from unlike machines, runtimes, target profiles, or workloads numerically comparable by normalization or a synthetic score.
- Defining target-specific numeric budgets before the applicable target design supplies them.
- Treating elapsed time, allocation behavior, or artifact size as part of message semantics.
- Treating a conformance pass as proof of linguistic, cultural, legal, accessibility, product, or translation quality.
- Treating a performance pass as proof of semantic conformance or security.
- Reserving public CLI commands, package names, report file paths, dashboard products, or wire encodings.
- Collecting hostnames, usernames, repository paths, environment variables, credentials, or other unrelated machine data.
- Benchmarking Provider, TMS, or network service latency as if it were deterministic Locale Compiler or production execution cost.
- Defining the logical resource limits owned by 018, 023, and the applicable component. Resource admission and physical measurement remain separate.

## Ownership and Dependencies

026 owns verification and measurement semantics. It consumes normative behavior from other designs and produces evidence about implementations of that behavior.

| Area | Responsibility relative to this design |
| --- | --- |
| Component and target designs | Own semantic operations, fixture meaning, phase/cost vocabulary, interval boundaries, workload dimensions, required cases, checksum observations, concrete data structures and memory lifetimes, component fast paths, and target-specific budget values |
| 017 shared artifacts | Owns the eventual wire encoding, version admission, canonical identity, and migration of shared conformance and measurement artifacts defined semantically here |
| 018 security and provenance | Owns trust, signatures, actor authority, sensitive-data policy, and whether evidence is authenticated for Release use |
| 019 project graph and Findings | Owns the common Finding envelope, dependency causes, evidence indexing, invalidation, queries, and scheduling |
| 020 planning and linking | Owns reachability, selected definitions, delivery placement, and pruning semantics used by output-size and finite-delivery fixtures |
| 023 execution specification | Owns logical message evaluation, values, functions, parts, locale services, diagnostics, failure behavior, and resource limits tested here |
| 024 target and export specification | Owns Target Profiles, capabilities, generated output semantics, artifact relationships, Runtime/native paths, and target-output identity |
| 025 Release specification | Owns Deployment Compatibility Groups, Release identity, publication, activation, execution admission, and the Release conditions that may require evidence |
| 026 common verification | Owns conformance campaigns, common evidence meaning, cross-component performance implementation principles, profiling/benchmark separation, measurement categories and units, comparison admission, baseline lifecycle, budget evaluation, and common reporting outcomes |
| 027 reference Runtime | Produces reference physical execution and measurement evidence; it does not redefine common conformance or comparison rules |
| Platform designs | Own target fixtures, allowed physical engines, runner classes, platform-specific budget values, and any target-specific measurement limitations |
| Product workflow | Owns commands, repository paths, CI scheduling, evidence retention services, dashboards, and user-facing presentation |

The dependency direction is one-way:

```text
semantic specification -> fixtures and claimed capabilities -> conformance evidence
physical operation specification -> owner benchmark result -> measurement evidence
measurement evidence + comparison policy -> comparison or budget outcome
```

A measurement outcome never becomes an alternate semantic authority. A budget may block a CI or Release decision, but it does not modify a Message Intent, selected artifact, Target Profile, formatted value, Finding, or resource-limit result.

### Relationship to product-owned benchmark schemas

[001](./001-ox-mf2-toolchain-foundation.md) requires checked structured benchmark results to remain product-owned because parser, formatter, linker, resolver, Runtime, and target workflows have different phase relationships and payloads. 026 preserves that decision.

An owner result is authoritative for:

- its `schemaVersion` and benchmark-profile revision;
- its closed phase/cost and metric vocabulary;
- exact interval start and end;
- permitted nesting or overlap;
- required-case matrix;
- fixture and workload semantics;
- semantic checksum observation;
- operational failure behavior; and
- any owner-specific relationships among records.

026 admits a result only through a versioned **Measurement Projection** registered for that exact owner schema and benchmark-profile revision. The projection:

- copies owner identities and tokens without renaming them;
- preserves canonical physical quantities exactly and admits raw-clock conversion only through the declared revision-`"0"` Duration Conversion Mode;
- supplies one common category and operation class without replacing the owner phase/cost;
- maps owner workload and environment facts into their common fields;
- retains a reference to the complete owner result;
- rejects an absent, ambiguous, lossy, or revision-mismatched mapping; and
- never manufactures missing raw samples, checksums, interval metadata, or environment facts.

An owner result that cannot be projected remains valid for its product-local purpose. It is simply ineligible for common comparison, budget, or cross-target claims.

### Relationship to component conformance suites

026 also does not absorb component suites such as the 015 Project Profile Resolver Conformance Suite. A component suite remains authoritative for its own construction, input, semantic, and failure cases. 026 defines a **Conformance Campaign** that can select exact suite revisions, execute or import their results, and prove cross-component or cross-target relationships.

Common fixtures owned by 026 are limited to relationships no single component owns, including:

- capability claims backed by applicable cases;
- preservation of common Finding meaning across projections;
- logical execution equivalence across physical engines;
- Browser/SSR hydration render equivalence;
- Release-consistent target combinations; and
- measurement-projection and comparison integrity.

## Design Principles

### Normative language

This design distinguishes requirements from implementation guidance.

- **MUST**, **MUST NOT**, and **required** identify normative conditions for evidence admission, semantic integrity, determinism, safety, or comparison correctness. Violating one produces the applicable conformance failure, invalid result, or incomplete evaluation.
- **SHOULD** and **SHOULD NOT** identify the normal performance-design direction. A conforming implementation may deviate when its owning design records the reason and applicable measurement evidence.
- **MAY** identifies an optional implementation choice.
- Preferences, candidates, ordinary expectations, and examples without one of the preceding requirement terms are non-normative guidance.

The Japanese translation uses the corresponding meanings of 「しなければならない」, 「すべき」, 「してもよい」, and 「例・指針」. Normative strength does not change between translations.

### Prove meaning before measuring speed

Every measured operation produces the semantic outcome expected by its owner fixture. Duration or memory samples are discarded when the semantic checksum, expected checked/blocked state, logical output, or required Finding differs.

A fast incorrect result is a failed benchmark operation, not a successful performance sample.

### Preserve owner vocabulary

The component that defines an operation is the only authority allowed to name and delimit it. A common category is an additional reporting dimension, not a replacement phase.

For example:

```text
owner:     015
phase:     profile_locale_resolve
cost:      locale_canonicalization
category:  toolchain
operation: component
metric:    wall_duration
```

The common model may group this record in a report, but it cannot call it `parse`, merge it with policy resolution, or infer its cost by subtracting another interval.

### Admit comparability explicitly

Two measurements are not comparable merely because their metric units match. Comparison requires one admitted Comparison Profile and compatible:

- owner operation meaning;
- fixture and workload;
- semantic result;
- subject and artifact set;
- build configuration;
- execution state;
- locale-service inputs where applicable;
- measurement method;
- sampling policy; and
- environment class.

An incompatible pair produces `not-comparable` with typed reasons. It never produces a misleading percentage.

### Retain observations; derive decisions

Raw admitted observations are immutable evidence. Baseline selection, statistical summaries, differences, ratios, regressions, and budget decisions are derived records that reference their exact inputs.

Changing a threshold or selecting a new baseline creates a new evaluation. It does not rewrite previous observations.

### Use exact quantities

Canonical measurement meaning uses integer nanoseconds, octets, and counts. Signed changes use a sign plus unsigned magnitude. Ratios retain exact numerator and denominator. Floating-point milliseconds, percentages, throughput, and humanized sizes are presentation only.

### Instrument early; gate deliberately

The first implementation of a component records the stable identities, semantic checksum, exact quantities, samples, and environment needed by later comparison. Initially, physical values may be observational while CI gates schema, required cases, interval integrity, and deterministic output.

Numeric gates activate only after a reviewed Runner Class Specification, valid Runner Qualification Evidence, Statistic Selection, baseline lifecycle, Performance Budget tolerance, and workflow disposition exist.

### Remove work before accelerating work

Performance work SHOULD follow this order. An owning design may use a different order when it records the reason and applicable measurement evidence:

1. remove duplicate computation and repeated parsing or normalization;
2. avoid optional analysis, metadata, diagnostics, and output that the caller did not request;
3. improve algorithms, incremental invalidation, and cache identity;
4. align ownership and memory lifetime;
5. improve data layout, capacity planning, and output construction;
6. reduce host-language boundary crossings and transferred materialization;
7. add bounded coarse-grained parallelism; and
8. specialize instructions, vectorize, or introduce isolated unsafe code.

A lower step SHOULD NOT be accepted merely because it improves a microbenchmark while an earlier step still performs avoidable logical work in the same production path. An exception requires an owning decision and evidence over the applicable production surface.

### Make the common path pay only for requested behavior

The common successful path uses cheap discriminators before expensive semantic work, processes contiguous literal or source-backed runs in bulk, and constructs final output directly where ownership permits.

Optional parts, source evidence, source maps, detailed diagnostics, traces, capability explanations, and debug statistics are not materialized unless the invoked operation or profile requests them. An optional feature may share semantic computation with the common path, but disabling it must avoid its feature-specific collection and output cost.

### Separate diagnosis, verification, and enforcement

Profiling identifies where time, allocation, contention, or materialization occurs. Benchmarking verifies the physical effect of a change with instrumentation disabled under an admitted workload. A regression gate retains an established property through an explicit baseline, runner class, statistic, and tolerance.

No stage substitutes for another:

- a profiler observation does not establish a benchmark value;
- a faster benchmark result does not explain its cause;
- a one-time improvement does not establish a durable budget; and
- a performance pass does not prove semantic conformance.

### Do not invent cross-platform normalization

026 provides common categories and side-by-side reporting across Web, mobile, and native targets. It does not divide results by clock frequency, core count, synthetic benchmark score, or another guessed normalization factor.

Numeric cross-target comparison is admitted only for a deliberately paired experiment whose Comparison Profile names the permitted target differences. Otherwise the report is descriptive.

## Terminology

| Term | Meaning |
| --- | --- |
| **Verification Subject** | Exact implementation, component, target output set, execution path, or Release relation being tested or measured |
| **Owner Result** | Versioned, checked component- or target-specific conformance or benchmark result whose schema and semantic operation names remain owner-controlled |
| **Conformance Suite** | Immutable versioned set of fixtures, cases, expected semantic results, and applicability rules owned by one specification |
| **Conformance Campaign** | Exact selection of suite revisions, Verification Subjects, Target and Locale Service Profiles, and required cross-suite relations evaluated together |
| **Capability Declaration** | Versioned statement of capabilities an implementation or target execution path claims to provide |
| **Capability Evidence** | Conformance evidence binding claimed capabilities to exact applicable passing cases and subject identities |
| **Logical Result** | Canonical semantic result compared by conformance: checked artifact facts, Finding facts, text or parts output, diagnostics, or typed failure state rather than platform presentation bytes |
| **Measurement Projection** | Versioned, lossless mapping from one exact owner benchmark schema/profile revision into the common measurement model |
| **Measurement Profile** | Immutable collection rules for required metadata, measurement method, samples, warmup, execution state, reset/reuse state, and environment capture; it does not decide comparison compatibility or budget tolerance |
| **Measurement Case** | One exact owner phase/cost, fixture, variant, scale, subject, workload, execution model, metric, and measurement-method combination |
| **Observation Sample** | One ordinal measured value covering a declared positive repetition count after warmup |
| **Measurement Evidence Set** | Immutable admitted set containing common metadata and samples while retaining exact owner-result identity and semantic observations |
| **Performance Surface** | One physical inclusion boundary such as core computation, host-language projection, generated-output delivery, or an end-to-end workflow; memory coverage is expressed separately by a Memory Observation Domain |
| **Memory Observation Domain** | Versioned descriptor of the observer, included and excluded storage classes, runtime/process coverage, and reuse state for one memory measurement |
| **Measurement Method Descriptor** | Versioned description of a metric provider, observation domain, event taxonomy, counting/conversion semantics, concurrency coverage, and overflow behavior |
| **Sample Aggregation Kind** | Declared meaning of one sample value: `batch_total`, `peak_over_batch`, `terminal_value`, or `deterministic_value` |
| **Artifact Set Scope** | Checked artifact collection measured by an artifact-size or delivery-topology case: one artifact, one Delivery Unit, an initial/eager closure, or a complete output set |
| **Execution State** | Independent process, engine, preparation, cache, runtime-compilation, managed-heap, scratch-reuse, and output-buffer-reuse facts for one case |
| **Scratch Workspace** | Component-owned reusable temporary storage with an explicit reset boundary that cannot be referenced by the resulting immutable artifact |
| **Profiler Observation** | Diagnostic hierarchical span, allocation, contention, or materialization record used to locate cost; it is never Measurement Evidence, while a separate uninstrumented owner benchmark for the same operation may produce Measurement Evidence |
| **Profiling Build** | Explicit non-default build or execution mode that enables diagnostic instrumentation and is distinct from the uninstrumented build used for performance comparison |
| **Environment Class** | Versioned comparison-relevant machine, OS, runtime, toolchain, build, and measurement-method facts selected by a Comparison Profile |
| **Statistic Selection** | Immutable choice of exact value or order statistic, minimum sample count, and deterministic-measurement requirement used to derive one scalar from an evidence set |
| **Comparison Profile** | Immutable rules that decide whether two evidence sets are compatible, select a Statistic Selection, and derive a difference and ratio; it does not own budget tolerance or workflow disposition |
| **Cross-Platform Report Profile** | Immutable descriptive selection, grouping, ordering, and context rules for side-by-side evidence that produces no numeric comparison or budget decision |
| **Baseline** | Explicit immutable reference to an admitted Measurement Evidence Set selected for a named comparison scope |
| **Performance Budget** | Versioned target- or product-owned upper, lower, range, exact, or baseline-relative requirement that owns its limits, tolerances, and advisory/gating disposition |
| **Runner Context** | `local-uncontrolled`, `controlled-unqualified`, or `qualified` status recorded for the environment that produced an observation |
| **Runner Qualification Evidence** | Immutable result proving whether a runner instance satisfies one versioned Runner Class Specification and its noise, drift, control, and validity rules |
| **Evaluation Outcome** | Structured `pass`, `warn`, `fail`, `not-comparable`, `unbaselined`, `incomplete`, or `invalid-evidence` result that references all evidence and policy inputs |
| **Logical Render Equivalence** | Equality or explicitly bounded equivalence of canonical text/parts and diagnostics for the same selected message, locale context, values, and execution specification |

## Architecture

![Intlify four verification layers](./assets/026-intlify-verification-layers.svg)

The architecture has four verification layers.

### 1. Owner-defined semantics and measurement points

Each owning specification defines what is being tested or measured. It owns semantic fixtures, product phases and costs, interval boundaries, valid nesting, workload dimensions, deterministic observations, and required cases.

This layer prevents a common reporting system from guessing where resolver construction, linker placement, artifact loading, Runtime preparation, or hot formatting begins and ends.

### 2. Owner-produced checked results

Each harness emits a checked result under its own versioned schema. A result must be complete for the invoked profile: malformed input, a missing required case, a partial prefix after failure, an unknown phase/cost, or a semantic checksum mismatch invalidates the result.

Console output, Criterion reports, Markdown tables, browser traces, and profiler files may accompany the checked result, but they are not automatically admitted as common evidence.

### 3. Common evidence admission

026 validates the exact Measurement Projection or conformance import adapter, preserves the owner result reference, and creates:

- semantic Conformance Evidence;
- a Conformance Campaign Evaluation;
- physical Measurement Evidence;
- positive Capability Evidence derived from complete applicable passing conformance cases; or
- a cross-target Logical Render Equivalence Evaluation whose `equivalent` outcome can serve as positive evidence.

Evidence admission checks integrity and internal consistency. It does not yet declare a performance regression or budget pass.

### 4. Comparison, budget, and reporting

A Comparison Profile selects compatible evidence and derives exact statistics. A Performance Budget may then evaluate those statistics and owns its tolerance and workflow disposition. A Cross-Platform Report Profile separately produces descriptive side-by-side rows without numeric comparison. Structured reports expose both decisions and reasons, while human presentation may render milliseconds, MiB, ratios, trends, and charts where admitted.

The report must keep these result kinds visually and structurally distinct:

| Result kind        | Question answered                                                       |
| ------------------ | ----------------------------------------------------------------------- |
| Conformance        | Did the subject preserve the required semantics?                        |
| Capability         | Is a claimed capability supported by applicable passing evidence?       |
| Measurement        | What physical value was observed under this exact case and environment? |
| Comparison         | Are these observations compatible, and how do they differ?              |
| Budget             | Does one compatible statistic satisfy an explicit requirement?          |
| Render equivalence | Did two execution paths produce the required same logical result?       |

## Performance Implementation Architecture

This section defines cross-component constraints that keep Intlify implementations measurable and make later optimization less likely to require an architectural rewrite. It does not prescribe one allocator, collection library, scheduler, binary layout, or host binding. Each component owns those physical choices and demonstrates them through its owner result.

### Performance surfaces

Performance is observed across distinct surfaces rather than reduced to one core-operation score. Revision `"0"` uses the following closed surface tokens.

| Token | Surface | Includes | Required separation |
| --- | --- | --- | --- |
| `core` | Core computation | Parse, normalize, resolve, link, plan, prepare, or evaluate inside one physical implementation | Excludes host projection and workflow I/O unless the owner interval explicitly includes them |
| `host_boundary` | Host boundary | Encoding, decoding, marshaling, native/managed calls, host-object materialization, and result projection | Reports transferred bytes, call count, materialization, and duration independently from the underlying core operation where measurable |
| `product_workflow` | Product workflow | Repository discovery, file I/O, scheduling, incremental lookup, component invocation, and checked result assembly | Keeps component intervals available instead of replacing them with only one end-to-end duration |
| `generated_output` | Generated output and delivery | Payload, packaging, compression, generated files, Delivery Units, eager-load closure, and required load requests | Uses one checked Target/Release artifact relation rather than filename inference |

Every Measurement Case maps to exactly one surface for one metric. An owner may measure the same logical operation on multiple surfaces as distinct cases, such as `core` duration and `host_boundary` duration.

If an operation crosses a host-language boundary, a promoted performance profile normally includes both a core-only case and a boundary-inclusive case. If a target generates deployable output, the profile includes both byte weight and delivery topology when both can affect application cost.

### Memory observation domains

Memory is not a Performance Surface. A memory or allocation case chooses the surface whose physical work is enclosed and additionally references one versioned Memory Observation Domain:

```text
MemoryObservationDomain {
  identity and revision
  observer or allocator identity
  included and excluded storage classes
  included harness, runtime, locale data, and shared libraries
  process, thread, worker, and child-process coverage
  allocation ownership and reuse state
}
```

The descriptor may include immutable artifacts, invocation scratch, worker scratch, shared caches, output buffers, and host projections. Process-wide observations state that broader scope rather than pretending to identify one lifetime class. A memory case cannot be admitted without the applicable descriptor, and cases with incompatible descriptors are not comparable unless a versioned compatibility rule proves equivalent observation meaning.

For example, peak live bytes during a host projection use `surface: host_boundary` plus the applicable host/native Memory Observation Domain. Peak live bytes for a component-only resolver invocation use `surface: core`. This preserves both the execution boundary and the memory coverage.

### Memory lifetime classes

Components classify storage before selecting an allocation technique.

| Lifetime class | Intended use | Cross-component requirement |
| --- | --- | --- |
| Immutable artifact | Checked Profile, linked message, plan, prepared message, Runtime artifact, or evidence retained after an invocation | Owns or safely shares its storage and never references resettable scratch |
| Invocation scratch | Temporary vectors, maps, strings, indexes, and queues for one operation | Has an explicit reset boundary and cannot escape in the result |
| Worker scratch | Reusable temporary state for repeated independent work on one worker | Is worker-owned rather than concurrently mutated by unrelated workers |
| Shared cache | Reusable immutable or synchronized state across operations | Has explicit identity, invalidation, entry and resident-byte limits, and deterministic uncached equivalence |
| Output buffer | Text, parts, serialization, or generated-code destination | Distinguishes caller-owned reusable output from an owned convenience result whose capacity leaves the producer |

A Scratch Workspace is preferred when ordinary collections and buffers can retain useful capacity across invocations. An arena is considered only when many objects share one demonstrable lifetime and bulk reset materially reduces measured allocation cost. Arena allocation is not a product-wide requirement and must not hold values that require independent destruction or outlive its reset boundary.

Workspace behavior follows these rules:

- `clear`-equivalent reset retains capacity; shrinking is explicit rather than part of the common path;
- no reference into scratch survives reset or enters an immutable artifact;
- workers do not share mutable scratch merely to reduce allocation count;
- capacity hints derive from admitted input or observed workload, are bounded, and are not copied blindly from an attacker-controlled maximum;
- pathological high-water capacity can be released by an explicit policy outside the latency-sensitive path; and
- owner diagnostics can expose capacity high-water marks and growth counts before those become common metrics.

An API that transfers an owned output buffer to its caller does not claim capacity reuse for the next call. A component that requires repeat-call output reuse provides a caller-owned or explicitly recyclable output path in addition to any owned convenience API.

### Common-case fast paths and optional work

The common successful path minimizes the number of semantic operations before considering instruction-level optimization.

- Cheap byte, tag, ID, capability, or state discriminators select a narrow parser or evaluator path before expensive recognition.
- Contiguous source-backed or literal runs are processed in bulk rather than one node, character, or host call at a time.
- Normalization, parsing, sorting, graph construction, and validation results are reused within their valid revision instead of recomputed by adjacent phases.
- Final output is constructed directly when doing so preserves the required ownership and checked failure semantics.
- Empty diagnostics, optional parts, source evidence, source maps, debug statistics, traces, and capability explanations are not allocated on a successful path that did not request them.
- Optional feature preflight is cheaper than the avoided feature work and is itself benchmarked for workloads where the feature is absent and present.

Fast-path selection cannot weaken admission, resource limits, deterministic output, or semantic checks. A cheap discriminator identifies candidates; it does not replace the parser or semantic decision that proves the result.

### Data layout, lookup, and identity

After input admission and canonicalization, frequently repeated processing prefers compact IDs and dense storage over repeated string hashing and pointer-heavy traversal where the domain is finite.

- Dense `0..N` IDs normally index contiguous vectors, compact tables, or bitsets.
- Canonical strings are normalized and interned or assigned an ID once per valid revision rather than repeatedly in an inner loop.
- An unordered internal lookup never determines artifact, Finding, evidence, or report order; freeze or projection establishes canonical order explicitly.
- A fast process-local hash may be used for admitted internal keys, but it does not become an artifact identity, integrity digest, wire value, or trust decision.
- Untrusted external strings are not inserted without admission and resource bounds into a predictable non-cryptographic hash domain.
- Small inline collections or compact strings are introduced only when measured cardinality and value length justify their size and branch trade-offs.
- Array-of-structures remains appropriate when all fields are consumed together; structure-of-arrays is considered only for measured large scans over a field subset.

Physical record size and alignment may have regression tests, but an in-memory language layout is never used as the shared wire encoding merely because it is compact locally.

### Bounded parallel and incremental work

Core component APIs remain synchronous and scheduler-neutral unless their owning design requires another execution model. Workflow and binding layers schedule independent files, profiles, locales, Delivery Units, or batch requests.

Parallel execution:

- uses coarse independent units rather than fine-grained parallelism inside one small message;
- preserves deterministic input/result association and canonical merge order;
- gives each worker its own Scratch Workspace;
- bounds a batch by item count, admitted input bytes, and predicted peak memory;
- avoids parallel startup when fixed scheduling cost dominates small workloads;
- records actual worker concurrency and contention where they affect the case; and
- does not create an implicit nested thread pool beneath a host scheduler.

Incremental execution keeps its bookkeeping off the one-shot common path. Cache identity includes every semantic input that can change the result, and cached and uncached execution must produce the same logical observation. Entry count alone is not a sufficient cache bound; resident bytes and any retained source/artifact bytes are also bounded and measurable.

### Host-boundary and output construction

Moving work to native code, a worker, or a binary representation is not itself a performance result. A boundary design minimizes crossings and independently accounts for transfer and host materialization.

- Batch APIs are preferred when independent per-item crossings would dominate core work.
- Compact handles, snapshots, or lazy accessors are candidates for large internal graphs; verbose host objects are produced only when required by the API.
- Heavy diagnostics, source maps, statistics, and trace payloads are opt-in.
- Encoding formats are compared end to end, including producer allocation, copies, transfer bytes, host decode, object materialization, and consumer access.
- A binary representation is not assumed to be smaller, zero-copy, or faster than a textual representation.
- Async or worker execution records queueing, transfer, execution, and result projection separately when those costs are independently observable.

Generated output is evaluated as a complete product surface. Reducing one file's compressed size does not establish improvement if it increases required files, Delivery Units, initial requests, initialization work, or retained memory for the same checked workload.

### Low-level specialization and unsafe code

Byte-search libraries, compiler-vectorizable loops, and other safe optimized primitives are preferred before architecture-specific code. Candidate low-level work is limited to measured byte-oriented operations such as delimiter search, escaping, UTF-8 validation/transcoding, digest calculation, and literal copying.

Locale-policy evaluation, fallback traversal, graph linking, Finding construction, and other branch-heavy semantic operations are not SIMD targets without contrary evidence.

An implementation-specific unsafe or architecture-specialized path requires:

- an existing scalar safe reference implementation;
- a stable benchmark showing material improvement on representative input distributions;
- isolation behind a safe facade and a portable fallback;
- documented safety and data-layout invariants;
- exact semantic differential tests;
- fuzzing and applicable memory/undefined-behavior checks; and
- a separate decision in the owning component specification.

026 does not authorize unsafe code merely by listing a possible optimization class.

### Enforceable performance guardrails

A stable performance rule is expressed as a lint, invariant test, feature-matrix test, size/alignment assertion, or benchmark fixture where practical instead of relying only on reviewer memory.

Candidate guardrails include:

- rejecting temporary formatting/collection patterns in allocation-sensitive modules when direct output construction is available;
- testing deterministic output independently from unordered internal lookup order;
- snapshotting allocation count or compact record size only under a pinned method and target layout;
- compiling ordinary and profiling feature sets separately;
- checking that profiler-only dependencies are absent from ordinary product artifacts; and
- recording all optimization, link, assertion, allocator, and feature differences between benchmark and release builds.

Guardrails prevent known regressions; they do not prove that an implementation is fast. Every rule that constrains ordinary implementation style must retain a correctness reason or measured performance justification and can be revised when the owning evidence changes.

## Profiling Specification

Profiling is a diagnostic capability for finding expensive spans, allocations, contention, and materialization. It is deliberately separate from common Measurement Evidence and Performance Budget evaluation.

### Build isolation and ordinary-build cost

Diagnostic instrumentation is guarded by an explicit non-default build feature or equivalent compile-time mechanism.

When profiling is disabled, an instrumentation call site contributes no required:

- span registration or dynamic label construction;
- runtime branch or enabled-state check;
- atomic operation;
- thread- or task-local access;
- allocation or trace-buffer write; or
- profiler dependency in a production target artifact.

The disabled and enabled variants have identical product semantics, public artifact formats, and logical results. Instrumentation cannot become necessary for cache identity, control flow, error handling, or cleanup.

Timing and allocation instrumentation may be separate features. In particular, replacing or wrapping a global allocator is a more invasive diagnostic mode and is not enabled merely because timing spans are enabled.

An implementation that claims zero required ordinary-build runtime work produces **Instrumentation Isolation Evidence**:

```text
InstrumentationIsolationEvidence {
  ordinary build identity
  profiling feature identity
  representative call-site set
  compile-time erasure method
  dependency-closure result
  artifact symbol, import, registry, and section result
  call-site code-generation result
  argument-evaluation result
}
```

The check MUST establish that the ordinary artifact has no linked profiler runtime or recorder, profiler-only symbol/import/registry/section/thread-local state, or evaluation of disabled span arguments. Representative call sites MUST contain no instrumentation branch, atomic operation, thread-local access, allocation, or recorder call. An owner may prove erasure through macro or transform expansion, compiler IR, normalized disassembly, linked-artifact inspection, dependency-graph inspection, and a side-effecting argument probe. Whole-artifact byte equality is not required.

This is build-isolation conformance evidence, not Measurement Evidence. An implementation that cannot prove compile-time erasure may still offer diagnostic profiling, but it cannot claim the 026 zero-required-runtime-work capability.

### Span registry and observation model

The owning component defines a finite versioned span registry aligned with, but not required to equal, its benchmark phase/cost registry. Hot call sites use static span IDs or static labels. Dynamic source text, paths, message content, locale values, and user-provided labels are not constructed or stored as span names.

A Profiler Observation is always a diagnostic record and is never Measurement Evidence. A separate uninstrumented owner benchmark may measure the same logical operation and produce an independently admitted Measurement Evidence Set.

A Profiler Observation conceptually contains one explicitly selected recorder mode:

```text
ProfilerObservation {
  profiler and registry revision
  Verification Subject and Profiling Build identity
  fixture and workload identity
  instrumentation capabilities
  thread, task, or worker context model
  recording:
    event-trace { ordered span events }
    | aggregate-table { ordered aggregate span records }
  truncation and profiler diagnostics
}

AggregateSpanRecord {
  span ID and parent span ID
  context identity
  complete occurrence count
  inclusive duration total
  self duration total | unavailable
  allocation and deallocation totals when enabled
  current and peak live-byte observations when enabled
  ordered diagnostic counters
}

ProfilerDiagnosticCounter {
  registry-defined counter ID
  unit: count | nanosecond | octet
  aggregation: total | maximum
  value
}

SpanCompletion =
  complete
  | cancelled
  | unwound
  | truncated
  | recorder-failed
```

An aggregate duration is the total across all complete occurrences; it is not an implicit average or maximum. Per-occurrence distributions require `event-trace`. A non-complete span is retained as diagnostic state where possible but is not mixed into complete occurrence totals. Its partial duration may be retained with its completion state. An incomplete child makes its parent's self duration unavailable, and a process failure that prevents report finalization is a profiler execution failure.

The physical representation may be a tree, table, or event stream consistent with the declared recorder mode. It MUST retain enough parent/context information to distinguish nested work from repeated sibling work and MUST report when limits truncate the observation. Different recorder modes or registry revisions are not treated as equivalent perturbation or directly comparable diagnostic shapes.

Recording favors fixed IDs and preallocated or reusable bounded storage. Human labels, sorting, aggregation across roots, and report formatting occur after the profiled workload where practical. The profiler reports its own recorder mode so a full event trace is not compared as though it had the same perturbation as aggregate counters.

Self duration is derived only for properly nested serial spans in the same declared timing context. When valid, it is the checked subtraction of the inclusive durations of direct serial children from the parent's inclusive duration. Overlap, underflow, an incomplete child, or incompatible clocks makes self duration unavailable rather than saturated or guessed.

Time from concurrent workers is not subtracted from a parent as if it were serial child time. Cross-thread or async relationships require explicit context propagation; otherwise they remain separate roots.

### Allocation profiling

An allocation-profiler observation states:

- observed allocation domain and allocator identity;
- whether the harness, runtime, dependencies, and profiler recorder are included;
- allocation, reallocation, and deallocation counting semantics;
- live-byte and peak-live calculation;
- thread/process coverage;
- counter overflow behavior; and
- known self-observation or sampling effects.

Profiler bookkeeping can itself allocate or perturb timing. Therefore allocation-profiler values locate likely cost and compare diagnostic shapes under the same profiler revision; they are not silently projected as uninstrumented allocation or duration evidence.

A process-wide allocator delta cannot be attributed to one span while unrelated threads allocate concurrently unless the observer can distinguish those domains. Such a recorder reports a process-wide overlapping observation or marks per-span attribution unavailable; it does not present the delta as thread-local self allocation.

An owner that needs gating allocation evidence defines a separate benchmark measurement method with explicit interval and allocation domain. Its result is admitted through the normal Measurement Projection.

### Bounded recording and privacy

Profiler state is bounded by registered span count, nesting depth, event/record count, retained bytes, and report size. Exceeding a bound produces explicit truncation or profiler failure; it does not silently wrap counters or discard an unknown subset.

Shared profiler output uses fixture and semantic identities rather than source content where possible. Source text, translations, absolute paths, dynamic labels, environment dumps, and credentials are excluded unless a separate local-only diagnostic mode explicitly requests them.

### Profiling-to-regression workflow

Performance changes follow this evidence flow:

```text
conforming representative fixture
  -> opt-in profiling build locates dominant self time/allocation/materialization
  -> implementation change
  -> same profiler setup confirms the diagnosed cost moved as expected
  -> uninstrumented owner benchmark quantifies the end-to-end effect
  -> admitted comparison evaluates compatible evidence
  -> advisory or gating budget retains the improvement
```

Real repository and product-path workloads are profiled before specializing a synthetic micro-case. A microbenchmark may then isolate the changed primitive and protect it from local regression, while an end-to-end benchmark verifies that the product surface improved.

Profiler reports may accompany owner results as diagnostic attachments. They are never admitted as common Measurement Evidence, are not used to select a numeric statistic, and cannot satisfy a Performance Budget.

## Common Evidence Model

The following shapes are semantic records. Their eventual wire representation, field tags, canonical encoding, digest framing, and migration are owned by 017.

### Evidence identity

Every evidence item binds:

- evidence kind and 026 specification revision;
- owner ID and owner specification revision;
- complete owner-result identity and integrity digest;
- Verification Subject identity and implementation revision;
- applicable suite, benchmark, fixture, and profile identities;
- applicable Target Profile, Locale Service Profile, Release, artifact, or capability identities;
- canonical result body; and
- creation tool identity.

Wall-clock creation time is optional report context. It never participates in semantic result equality, Measurement Case identity, comparison compatibility, or product cache identity.

An evidence digest identifies the complete immutable evidence body. It is not proof that the runner was trustworthy. Authentication and authorized use are separate 018-owned decisions.

### Conformance evidence

Conceptually:

```text
ConformanceEvidence {
  evidence identity
  campaign identity
  suite identity and revision
  case identity and fixture digest
  verification subject
  case result:
    applicable {
      outcome: pass | fail
      expected logical result identity
      observed logical result identity
    }
    | not-applicable {
      applicability rule identity
    }
  finding observations
  capability associations
}
```

An applicable case has only `pass` or `fail`. A `not-applicable` record has no pass/fail outcome and retains the exact applicability rule that excluded it. Environmental inability to execute is a campaign execution failure, not a semantic pass and not silently `not-applicable`.

`not-applicable` is allowed only when a suite rule proves that the case is outside an unclaimed optional capability or target dimension. A Target Profile that requires the capability makes the case applicable.

### Campaign, capability, and equivalence records

The campaign-wide result is distinct from an individual case record:

```text
ConformanceCampaignEvaluation {
  campaign identity
  selected suite revisions
  ordered Verification Subjects
  ordered case-result references
  required relation results
  completeness inventory
  outcome: pass | fail | incomplete | invalid
  typed reasons
}
```

Capability Evidence is positive evidence and is produced only when the complete applicable required-case group passes:

```text
CapabilityEvidence {
  evidence identity
  capability declaration identity
  Verification Subject and build identity
  Target Profile and physical execution-path identity
  required case-group identity
  ordered complete applicable passing-case references
  applicable profile and suite revisions
}
```

A failed or incomplete capability claim remains represented by the Campaign Evaluation and applicable Findings; it does not produce partial positive Capability Evidence.

Logical-render comparison produces a structured evaluation for both positive and negative results:

```text
LogicalRenderEquivalenceEvaluation {
  equivalence relation identity
  left and right execution-observation references
  compared logical-result identities
  applied exact-equality or typed-variation rule
  outcome: equivalent | not-equivalent | incomplete | invalid
  typed reasons
}
```

Only `equivalent` can satisfy a positive Release evidence requirement. The other outcomes remain immutable structured results for diagnosis and policy decisions.

### Measurement evidence

Conceptually:

```text
MeasurementEvidenceSet {
  evidence identity
  measurement profile identity
  projection identity
  owner result identity
  verification subject
  build identity
  environment observation
  ordered measurement cases [
    owner phase and cost
    common category and operation class
    performance surface
    artifact set scope when applicable
    memory observation domain when applicable
    fixture, variant, scale, execution model
    execution state
    workload identity and logical work vector
    metric, measurement method descriptor, and sample aggregation kind
    interval identity and overlap facts when applicable
    warmup policy
    ordered observation samples
    semantic observation identity
  ]
}
```

The common evidence retains samples rather than only a precomputed average. A product result may contain additional owner-specific aggregates and relationships, which remain available through its retained identity.

### Comparison and budget evidence

Conceptually:

```text
ComparisonEvaluation {
  comparison profile identity
  comparison mode
  baseline evidence and case identity
  candidate evidence and case identity
  result:
    comparable {
      Statistic Selection identity
      ordered pair references when paired
      exact baseline and candidate statistics
      signed difference
      exact ratio
    }
    | not-comparable {
      typed compatibility reasons
    }
}

BudgetEvaluation {
  performance budget identity
  comparison or direct measurement identity
  Statistic Selection identity for direct measurement
  workflow disposition: report | warn | block
  result:
    evaluated {
      evaluated limit
      tolerance when applicable
      outcome: pass | warn | fail
    }
    | unavailable {
      outcome: not-comparable | unbaselined | incomplete | invalid-evidence
      typed reasons
    }
}
```

Neither record copies observations and then treats the copy as new evidence. It references exact admitted inputs.

## Conformance Specification

### Suite closure and reproducibility

Every promoted Conformance Suite is a finite, immutable, content-addressed closure. Its index explicitly references:

- the suite specification and schema revisions;
- every case manifest and fixture payload;
- expected logical result observations;
- applicable capability and target selectors;
- canonical observation codecs;
- required-case groups;
- permitted implementation parameters; and
- all transitive suite-owned data.

The suite does not discover authoritative cases by scanning a directory, following a changing `latest` reference, or consulting the network. A runner verifies every referenced digest before executing a case.

External standards fixtures may be included by pinned revision and digest. A repository URL or standards version label alone is not a fixture identity.

### Case identity and parameters

A conformance case has a stable opaque ID within one suite revision. Its semantic identity includes:

- fixture bytes and fixture role;
- exact semantic specification revision;
- input artifacts and values;
- requested locale and definition locale when applicable;
- Target Profile and Locale Service Profile requirements;
- output mode;
- expected successful or typed failure result;
- required and excluded capabilities;
- resource-bound vector where applicable; and
- canonical observation codec.

Case parameters are finite and enumerated in the suite. A runner may shard or reorder cases, but execution order and worker assignment cannot change an individual case result.

A generated family records the generator identity, revision, parameters, and digest of every realized case. A runner cannot substitute a new generated input under an existing case identity.

### Logical result comparison

Conformance compares canonical logical results, not private memory layout, pointer identity, thread schedule, filesystem path, JavaScript object prototype, DOM serialization, native view hierarchy, or human reporter text.

Depending on the owning specification, the logical result may include:

- checked artifact fields and canonical identity;
- an ordered Finding set and typed dependency causes;
- selected Intent, requested locale, definition locale, and Message Artifact identity;
- plain text;
- ordered structured parts, annotations, markup, fallback values, and bidi facts;
- stable diagnostic code, category, severity, typed data, and source identity/span;
- typed success, blocked, recoverable, or failure classification;
- resource-counter observations at exact and first-over limits; or
- capability-admission facts.

Private implementation details are excluded unless the owning semantic specification makes them observable.

### Finding projection conformance

019 owns the common Finding model. 026 verifies that CLI, editor, agent, build, Runtime, and target-specific projections preserve its meaning.

A projection case compares at least:

- stable Finding code and owning specification;
- severity and blocking effect;
- affected semantic entities;
- typed dependency cause;
- primary and related source identities;
- canonical UTF-8 spans before host-position conversion;
- structured parameters;
- suggested action or edit identity when present; and
- deterministic ordering and truncation state.

The following may vary only when the projection specification explicitly allows it:

- human-readable message localization;
- path presentation relative to an editor or workspace;
- line/column or UTF-16 conversion derived from the same canonical span;
- terminal color and layout; and
- UI grouping that retains every underlying Finding identity.

A projection must not drop a blocking Finding, change severity, erase truncation, merge distinct semantic entities, or convert an unknown required field into a guessed default.

### Capability declaration and evidence

A Capability Declaration is not self-certifying. For every capability claimed by a Verification Subject:

1. the applicable Target Profile or implementation specification selects the required suite and case group;
2. the campaign resolves every required case after considering declared dependencies and exclusions;
3. every applicable case passes against the same subject build and compatible service profiles;
4. no required case is skipped, missing, stale, or executed under a different implementation identity; and
5. the resulting Capability Evidence records the complete case set and any allowed variation.

Capability Evidence is valid only for its exact:

- implementation and build identity;
- capability-set revision;
- Target Profile;
- physical execution-path kind;
- artifact and Runtime ABI revisions;
- Locale Service Profile and locale-data identity where applicable;
- suite and fixture revisions; and
- required host/runtime versions.

A change to any comparison-relevant member invalidates the affected evidence through 019-owned dependency processing. Passing a smaller capability subset cannot be reused as evidence for a superset.

An unsupported optional capability is represented by its absence from the declaration. An implementation that claims the capability and fails its case is non-conforming; it cannot relabel the case as non-applicable.

### Execution conformance

023 supplies normative execution behavior. Its common campaign covers at least:

- simple text and interpolation;
- declarations, selectors, exact variants, and fallback variants;
- every promoted portable Message Value and function;
- requested-locale and definition-locale separation;
- plain-text and structured-parts output;
- bidi and direction behavior;
- missing, unexpected, and incompatible arguments;
- recoverable evaluation diagnostics and fallback values;
- inert markup and allowlisted projection inputs;
- exact and first-over resource bounds;
- cached and uncached semantic equality;
- locale-bound isolation;
- mixed-release and incompatible-artifact rejection through 024–025; and
- deterministic behavior under admitted concurrency.

A runtime-backed engine and an ahead-of-time or platform-native engine may use different physical representations. They conform when their applicable logical observations satisfy the same campaign and their Capability Declarations accurately describe any unsupported features.

### Locale Service Profile conformance

A pinned Locale Service Profile requires exact canonical logical output for the same complete input, implementation/data revisions, and applicable execution specification.

A platform-managed profile may admit only variation enumerated by its profile and fixture codec. Allowed variation is represented through typed alternatives or relations, not a free-form statement that results may differ.

Examples of admissible typed variation include:

- one of a finite set of spacing code points;
- an explicitly allowed numbering-system result;
- a platform-owned time-zone-name variant; or
- a declared native collation behavior when collation is in scope.

Examples that are not admissible include:

- arbitrary string inequality;
- missing or reordered message parts;
- a different selected Message Artifact;
- a changed requested or definition locale;
- a dropped diagnostic; or
- a semantic downgrade hidden as platform variation.

### Logical render equivalence

Logical Render Equivalence compares two complete execution observations with the same:

- Message Intent revision;
- selected Message Artifact and definition locale;
- requested locale;
- portable argument values;
- function and capability inputs;
- applicable message and execution specification;
- output mode; and
- declared locale-service relationship.

For ordinary cross-engine comparison:

- pinned compatible Locale Service Profiles require exact logical-result equality;
- platform-managed profiles use only the explicit typed variation relation above; and
- an unsupported required feature is a capability-admission failure rather than approximate output.

For a Browser/SSR hydration relation, equivalence is stricter. Server output and the client initial render must have the same effective requested locale, selected Message Artifact, definition locale, and exact logical text or parts required by the hydration fixture. A platform-managed profile cannot weaken this relation. If the pair cannot guarantee it, 024–025 must reject that hydration-coupled target combination before Release Assembly.

The comparison excludes physical DOM node identity, framework component instances, HTML serializer choices not represented in logical parts, and native view objects. Framework designs may add a later projection-specific hydration check after this logical check.

### Conformance campaign outcomes

A campaign produces one of:

- `pass` — every required applicable case passed and every required relation was satisfied;
- `fail` — at least one applicable case or relation produced a different logical result;
- `incomplete` — required evidence was missing, stale, corrupt, or could not be executed; or
- `invalid` — the campaign, suite closure, subject declaration, or result was malformed or inconsistent.

Only `pass` is positive conformance evidence. `incomplete` is never converted into `pass` because a platform or runner was unavailable.

One failing case does not permit a result prefix to certify other capability groups unless the suite explicitly partitions those groups and each partition has independently complete evidence.

## Measurement Specification

### Measurement Case identity

A Measurement Case is identified by the following semantic dimensions:

- owner ID and owner result-schema revision;
- owner benchmark-profile revision;
- owner phase and cost;
- common category and operation class;
- performance surface;
- Artifact Set Scope where applicable;
- Memory Observation Domain where applicable;
- interval boundary identity when the metric observes an interval;
- fixture ID and fixture revision;
- variant and scale;
- Verification Subject kind and stable identity;
- applicable artifact, Target, Release, and Locale Service Profile identities;
- execution model and complete Execution State;
- concurrency and worker model;
- workload profile and logical work vector;
- metric, Measurement Method Descriptor, and Sample Aggregation Kind; and
- Measurement Profile revision.

Human labels, generated time, sample values, VCS branch name, absolute path, worker ID, and implementation revision being compared do not enter the case identity.

The implementation revision is retained on each evidence set so different implementations can be compared under the same case.

### Common categories and operation classes

026 defines closed revision-`"0"` common categories. An owner-specific phase/cost maps to exactly one category, operation class, and Performance Surface for each metric.

| Category | Initial operation classes | Meaning |
| --- | --- | --- |
| `toolchain` | `component`, `workflow`, `startup`, `io` | Compiler, resolver, parser, linker, exporter, codec, command, and other build/tooling work |
| `artifact_size` | `payload`, `packaged`, `transfer`, `installed` | Exact size of generated localization outputs or shipped execution components under one declared representation and separate Artifact Set Scope |
| `delivery_topology` | `generated_file`, `delivery_unit`, `initial_load_request`, `complete_load_request` | Exact count of generated or required loading units under checked Target/Release artifact relationships |
| `initialization` | `process`, `engine`, `release_admission`, `localizer`, `binding` | Startup work before artifact loading or message formatting, with each owned operation kept separate |
| `loading` | `io`, `decode`, `integrity`, `admission`, `registration`, `complete` | Retrieval and admission of an immutable manifest, delivery unit, locale artifact, or native resource |
| `formatting` | `preparation`, `cache_lookup`, `cold_text`, `hot_text`, `cold_parts`, `hot_parts`, `application_e2e` | Selected-message preparation and synchronous evaluation under an explicit cache/output state |
| `boundary` | `encode`, `decode`, `marshal`, `call`, `batch`, `materialize`, `application_e2e` | Host-language, process, worker, or managed/native boundary work kept distinct from the underlying core operation |
| `memory` | `peak_live`, `retained_live`, `peak_rss`, `cache_resident`, `artifact_resident` | Memory observations whose provider and lifetime are explicit |

The categories are for common reports and budget selection. They do not imply that every target implements every operation class.

Category and Performance Surface are independent dimensions. Category describes the kind of operation or cost; surface describes the physical inclusion boundary. For example, producer-side encoding alone may be `boundary / encode` on the `core` surface, while the complete encode-cross-decode-materialize path is `boundary / application_e2e` on the `host_boundary` surface.

A combined operation such as end-to-end loading may coexist with component observations. The relationship is declared through owner interval topology; its duration is not treated as the sum of separately sampled children.

### Metrics and units

Revision `"0"` admits these common metric meanings:

| Metric                        | Canonical unit | Value                          |
| ----------------------------- | -------------- | ------------------------------ |
| `wall_duration`               | nanosecond     | non-negative unsigned quantity |
| `payload_bytes`               | octet          | non-negative unsigned quantity |
| `packaged_bytes`              | octet          | non-negative unsigned quantity |
| `transfer_bytes`              | octet          | non-negative unsigned quantity |
| `installed_bytes`             | octet          | non-negative unsigned quantity |
| `peak_live_bytes`             | octet          | non-negative unsigned quantity |
| `retained_live_bytes`         | octet          | non-negative unsigned quantity |
| `peak_rss_bytes`              | octet          | non-negative unsigned quantity |
| `cache_resident_bytes`        | octet          | non-negative unsigned quantity |
| `artifact_resident_bytes`     | octet          | non-negative unsigned quantity |
| `allocation_count`            | count          | non-negative unsigned quantity |
| `reallocation_count`          | count          | non-negative unsigned quantity |
| `boundary_call_count`         | count          | non-negative unsigned quantity |
| `materialized_object_count`   | count          | non-negative unsigned quantity |
| `generated_file_count`        | count          | non-negative unsigned quantity |
| `delivery_unit_count`         | count          | non-negative unsigned quantity |
| `initial_load_request_count`  | count          | non-negative unsigned quantity |
| `complete_load_request_count` | count          | non-negative unsigned quantity |

The metric fixes the unit. A record cannot relabel milliseconds as nanoseconds or megabytes as bytes.

Throughput, operations per second, milliseconds per operation, MiB, percentage change, and compression percentage are derived presentation values. They are not canonical stored observations in revision `"0"`.

### Measurement Method Descriptors

Every admitted metric references one finite, versioned Measurement Method Descriptor:

```text
MeasurementMethodDescriptor {
  identity and revision
  metric and provider or observer identity
  included and excluded observation domain
  event taxonomy and counting or conversion semantics
  concurrency coverage
  overflow behavior
}
```

For `allocation_count` and `reallocation_count`, the descriptor defines zero-size allocation, in-place growth and shrink, profiler/observer self-allocation, deallocation relationships, and thread/process coverage. For `boundary_call_count`, it defines both endpoints, host-to-native/native-to-host/callback inclusion, and batch semantics. For `materialized_object_count`, it fixes an object-taxonomy revision and eager/lazy materialization rules.

Revision `"0"` compares such counts only when descriptor identities match or a versioned compatibility rule proves equivalent meaning. A value without a complete descriptor remains an owner-specific diagnostic and is not projected into a common metric.

### Owner-specific diagnostic metrics and promotion

An owner result may retain diagnostic metrics that are not common revision-`"0"` metrics, including:

- Scratch Workspace capacity high-water mark;
- output-buffer growth count;
- total allocated bytes;
- cache hit, miss, and eviction counts;
- lock-wait duration;
- actual peak worker concurrency;
- unchanged entities recomputed after an edit; and
- serializer section copies or intermediate bytes.

A Measurement Projection preserves such fields through the retained owner result but does not rename them into an inexact common metric. A diagnostic metric is promoted in a later 026 revision only when its observation point, unit, inclusion domain, overflow behavior, and cross-component meaning are stable enough for lossless projection and useful comparison.

Unsupported observation remains explicit. Implementations do not estimate a metric from another metric, infer reallocation from capacity, or substitute a profiler counter for an uninstrumented benchmark observation.

### Exact numeric representation

The semantic numeric domain is `0..=u64::MAX`. A wire representation must preserve every value exactly across Rust, JavaScript, JSON, WASM, C ABI, Swift, Kotlin, and other bindings. Until 017 fixes a canonical wire encoding, JSON-facing product schemas use the shortest unsigned decimal string for values that may exceed the JavaScript safe-integer range.

Duration is captured from a monotonic clock. The observer computes the interval in its raw clock domain before converting it to canonical integer nanoseconds. Here, exactness means that the stored integer is preserved losslessly; it does not claim that a physical clock has infinite precision.

Revision `"0"` admits the following declared conversion modes:

```text
DurationConversionMode =
  exact-integer-nanoseconds
  | round-to-nearest-ties-to-even
```

The Measurement Method Descriptor records the clock identity, clock resolution, and conversion mode. A floating-point millisecond clock or a tick period that is not an integral nanosecond may therefore be used through the deterministic rounding mode. A timer reading or conversion that is NaN, infinite, negative, reversed, or overflowing produces a measurement failure rather than a sample.

Comparison requires compatible clock, resolution, and conversion semantics. A gating Measurement Profile also requires its aggregate duration to be sufficiently larger than clock resolution through an explicit resolution relationship and a fixed repetition policy; 026 does not normalize unlike clocks or environments.

Differences use:

```text
SignedDifference =
  baseline-larger { magnitude: u64 }
  | equal { magnitude: 0 }
  | candidate-larger { magnitude: u64 }
```

Ratios use exact unreduced integers:

```text
Ratio =
  defined { numerator: u64, denominator: non-zero u64 }
  | undefined-zero-denominator
```

Reporters may round a displayed value. The exact quantity and ratio remain available in structured output and are the only inputs to a decision.

### Interval measurement

Every interval observation names an owner-defined boundary descriptor containing:

- stable boundary ID;
- owner phase and cost;
- metric;
- occurrence policy;
- first included stage marker;
- final included stage marker;
- ordered included and excluded stage markers; and
- permitted direct parent/child overlap.

The timer, allocator, or sampler starts and stops at those owner-defined markers. Fixture generation, input acquisition, benchmark setup, clock or allocator setup, semantic checksum encoding, result validation, reporting, and teardown are excluded unless the owner explicitly defines a broader operation that includes one of them.

Nested and overlapping intervals are reported independently. 026 never subtracts `parent - child` to invent an unmeasured cost.

A profiling span may share an owner phase/cost label with a benchmark interval, but it does not define that interval implicitly. The owner boundary descriptor remains authoritative, and changing profiler nesting does not move a benchmark timer or allocator boundary without an owner-profile revision.

Concurrent operations require an owner-declared occurrence policy:

- `single` — exactly one interval in one repetition;
- `sequential-aggregate` — a canonical ordered sequence is measured as one sample;
- `concurrent-wall` — one wall interval covers the complete concurrent operation; or
- `per-occurrence` — each occurrence is a separate Measurement Case dimension.

CPU-time summation across workers is outside revision `"0"`. It cannot be substituted for wall duration.

### Artifact-size and delivery-topology measurement

Artifact size is deterministic only when the measured representation and complete artifact set are fixed.

Representation stage and measured set are separate dimensions. Revision `"0"` uses:

```text
ArtifactSetScope =
  single_artifact
  | delivery_unit
  | initial_eager_closure
  | complete_output_set
```

The checked Target/Release relationship supplies the exact members and subject identity for that scope. Locale, `shared`, artifact kind, Delivery Unit, execution component, and locale-data attribution remain separate grouping facts.

The exact meanings are:

- `payload_bytes` — length of the artifact payload produced by the checked exporter, excluding metadata not in that payload;
- `packaged_bytes` — exact bytes of a named deterministic package or container representation;
- `transfer_bytes` — exact bytes after one named deterministic compression and framing profile;
- `installed_bytes` — exact regular-file bytes in the admitted installed artifact set, excluding filesystem block allocation unless a future metric names it separately.

A report must not label any of these as generic `bundle size`.

Size evidence records the applicable grouping and attribution:

- complete Release or Target Profile output set;
- initial/eager-load closure;
- concrete requested locale or `shared`;
- concrete Delivery Unit or `shared`;
- artifact kind;
- execution component;
- locale-data component; and
- Runtime-backed, ahead-of-time, or platform-native path.

The grouping derives from checked 024–025 artifact relationships, never filename parsing or content sniffing. Shared bytes are reported in a separate bucket unless a target-owned budget explicitly defines another attribution rule. A comparison uses the same grouping and attribution revision on both sides.

Compression evidence fixes algorithm, implementation revision, parameters, dictionary identity, and container framing. Compressed sizes produced under different profiles are not comparable.

Delivery-topology metrics use the same checked grouping and attribution:

- `generated_file_count` counts regular generated files in the named complete or initial/eager artifact set;
- `delivery_unit_count` counts distinct checked Delivery Units required by that set; and
- `initial_load_request_count` counts the target-defined load operations required to make the initial/eager closure ready under one fixed packaging and loader profile; and
- `complete_load_request_count` counts the target-defined load operations required to make the complete measured Target/Release output set ready under the same kind of fixed profile.

Directories, source inputs, debug outputs, source maps, and optional metadata are included only when the measured Target/Release relation includes them. Shared files and Delivery Units are counted once per measured closure, not once per referencing locale or message.

Both load-request metrics describe deterministic loader topology. They are not observations of live network traffic, retries, protocol multiplexing, cache state, or latency. A runtime network experiment uses a separate owner case and environment.

### Initialization measurement

Initialization observations keep these physical responsibilities separate:

- host process or wrapper startup;
- language binding initialization;
- Runtime Engine construction;
- Release or manifest compatibility admission;
- immutable function and locale-service registry construction;
- Localizer creation; and
- framework or application adapter initialization.

A target may expose only a subset. A complete application-start observation may include several components, but it is a separately named owner workflow and does not replace the component observations required by its profile.

Process startup uses `fresh_process`. Engine and Localizer construction normally use `in_process` unless their owner specifies otherwise. A warm reused process cannot be compared with a fresh-process baseline.

### Loading measurement

Loading observations distinguish:

- application- or repository-owned I/O;
- transfer or fetch when a deterministic local delivery fixture exists;
- decoding;
- integrity verification;
- artifact and ABI admission;
- registration into an immutable Runtime Engine;
- cache-hit validation/access; and
- complete ready-to-format loading.

Provider or TMS access is never part of production loading.

A network benchmark may exist for a host delivery integration, but it requires its own network environment and is not comparable with deterministic local artifact loading. It cannot become evidence for offline execution latency.

The end of `loading / complete` is the point at which the declared Localizer/delivery unit can perform the profiled synchronous format operation. Lazy loading of another Delivery Unit is a separate case.

### Host-boundary measurement

A host-boundary case identifies both sides of the boundary and separates, where observable:

- producer-side encoding or serialization;
- queueing or call setup;
- transferred payload bytes;
- boundary crossing count;
- consumer-side decoding;
- host-object or managed-value materialization;
- result projection; and
- complete boundary-inclusive operation.

`boundary_call_count` counts actual crossings made by the declared operation, including callback crossings when the profile includes them. A batch invocation is one crossing plus its declared item count; it is not relabeled as one logical item.

`materialized_object_count` requires an owner-defined object domain. For example, a host object, array, string, part, or wrapper may each be a counted object if the method declares that taxonomy. Values from different taxonomies or instrumentation revisions are not comparable.

A transfer case records representation and version, framing, copy/ownership mode where observable, batch size, input and output bytes, and whether consumer access is eager or lazy. A format described as binary, compact, shared, or zero-copy receives no special comparison status without these observations.

Core-only and boundary-inclusive cases use the same semantic observation for the same logical input. The boundary-inclusive result is not admitted when host projection drops diagnostics, changes part structure, or otherwise changes the logical result.

### Execution state

Cache temperature, process reuse, JIT warmup, and managed-heap state are independent dimensions. Every applicable case records:

```text
ExecutionState {
  processState: fresh-process | reused-process
  engineState: fresh | reused
  preparationState: absent | resident
  cacheState: disabled | empty | miss | hit
  runtimeCompilationState:
    not-applicable | ahead-of-time | interpreter |
    jit-cold | jit-warmed | platform-managed
  managedHeapState:
    not-applicable | natural | forced-collection-before-case |
    forced-collection-before-sample | declared-precondition
  scratchReuseState
  outputBufferReuseState
}
```

An owner Measurement Method Descriptor records any concrete JIT tier, warmup termination rule, garbage collector and configuration, heap-occupancy precondition, concurrent-GC behavior, and whether GC pauses are inside the measured interval. Different runtime-compilation or managed-heap states are not comparable unless a Comparison Profile explicitly permits and interprets the difference.

### Formatting measurement

Formatting cases must state:

- text or structured-parts output;
- complete Execution State;
- whether message preparation is included;
- cache state and cache capacity;
- argument shape and value classes;
- selected message complexity;
- requested and definition locale;
- Locale Service Profile and data identity;
- diagnostic/fallback path;
- output consumption or checksum behavior outside the interval;
- requested optional outputs and diagnostic detail;
- output-buffer ownership and reuse mode;
- Localizer reuse;
- concurrency and contention model; and
- batch/repetition size.

Revision `"0"` meanings are:

- `preparation` — convert one already admitted checked message into the physical prepared representation;
- `cache_lookup` — locate an existing prepared representation under an exact cache identity;
- `cold_text` / `cold_parts` — format when the declared prepared-message cache state is absent; preparation is included only when the owner boundary says so;
- `hot_text` / `hot_parts` — format with all declared required artifacts and prepared-message entries already admitted and resident;
- `application_e2e` — compiler-lowered application call through binding, handle resolution, evaluation, and host result projection.

In `hot_text` and `hot_parts`, **hot** describes only resident admitted artifacts and prepared-message cache state. It does not imply a reused process, a warmed JIT tier, or any particular managed-heap state. Those facts remain independent Execution State fields.

Cold and hot results must be semantically equal for the same logical input. That equivalence is checked before their physical values are admitted.

A hot benchmark cannot silently omit required locale-service work, argument admission, result construction, or diagnostics simply because the result is known to the fixture.

A case named `hot_text` or `hot_parts` has already admitted required artifacts and prepared-message state. Its measured interval does not perform filesystem/network I/O, message syntax parsing, locale canonicalization, fallback-graph construction, sorting, artifact admission, or global configuration mutation. If an implementation intentionally performs one of those operations during formatting, the owner uses a different operation class or explicitly includes and reports the work rather than calling the case hot.

### Memory measurement

Memory metrics are not interchangeable.

- `peak_live_bytes` and `retained_live_bytes` require an allocator-observation method with a declared Memory Observation Domain.
- `allocation_count` and `reallocation_count` require an allocator or runtime observer that defines allocation, growth, shrink, and zero-size behavior.
- `peak_rss_bytes` requires a process sampler, sampling cadence, platform API, process-tree inclusion rule, and interval.
- `cache_resident_bytes` measures exact cache-owned live storage under a declared full state.
- `artifact_resident_bytes` measures admitted artifact storage retained for the subject state.

Peak live bytes cannot be compared with peak RSS. Allocator-observed results from different allocator or instrumentation revisions are not comparable unless a Comparison Profile explicitly admits them.

Memory measurement records its Memory Observation Domain, including whether the harness, language runtime, JIT, shared libraries, locale data, and child processes are included. Missing measurement support is `unsupported-measurement`, not zero.

### Workload identity and logical work vectors

Every case references an immutable Workload Profile and records one logical work vector. The owner decides the meaningful dimensions. Typical dimensions include:

- source or artifact bytes;
- syntax nodes, messages, parameters, selectors, variants, and parts;
- locale occurrences and canonical locale count;
- requested locales and definition locales;
- Target Profiles, groups, delivery units, and artifact count;
- cache entries and resident bytes;
- Finding and diagnostic counts;
- concurrent workers or requests; and
- boundary calls and materialized host objects;
- generated files, Delivery Units, initial load requests, and complete load requests;
- requested optional features and diagnostic detail; and
- output bytes or parts.

Where a normative resource counter exists, the work-vector dimension uses the same counting point and unit. This makes scale visible without turning a resource limit into a performance claim.

The workload identity covers fixture revision, generator revision and parameters, logical input identities, and expected semantic observation. Reordering equivalent input may be a separate variant if the owner wants to prove order independence.

Two cases with different work vectors are not a direct regression pair. A scaling report may compare them only under an explicit scale-series profile and may not present the result as a same-workload speed regression.

### Representative workload matrix

An owner benchmark profile selects applicable cases from the following dimensions rather than reporting one undifferentiated average:

- fresh/reused process and engine states, absent/resident preparation, cache miss/hit, and each applicable runtime-compilation and managed-heap state;
- cache miss, cache hit, unchanged rerun, and one admitted local edit;
- fresh scratch allocation, bounded input-derived preallocation, and reset/reused Scratch Workspace;
- caller-owned reusable output and owned convenience output;
- small and large batches, including the threshold below which scheduling remains sequential;
- short common input, long literal/source runs, Unicode-heavy input, escape/syntax-heavy input, diagnostic-heavy input, and resource-limit boundaries;
- one large aggregate input and a realistic set of many small files or messages;
- synthetic primitive fixture and pinned real-repository or product-path fixture;
- core-only and host-boundary-inclusive operation;
- supported transfer representations and eager/lazy host materialization; and
- complete and pruned output sets with initial/eager delivery topology.

Not every component implements every dimension. Its owning design records applicability and the minimum required matrix. A benchmark claim names the exact case rather than generalizing a favorable variant to unmeasured workloads.

Preallocation uses a bounded hint derived from the admitted workload. Profiles do not use a logical resource-limit maximum as the initial capacity merely because that maximum is available.

### Semantic observations and benchmark integrity

Every duration case records a deterministic semantic observation for each measured sample. This may be:

- an owner-defined checksum over canonical logical output;
- an exact artifact/content digest already owned by the product;
- an expected typed blocked outcome plus canonical Findings; or
- a conformance Logical Result identity.

The observation excludes time, memory, sample ordinal, worker identity, pointer identity, environment metadata, and report time.

All repetitions within one sample and all samples in one evidence set must produce the same semantic observation unless the owner benchmark profile and applicable platform-managed Locale Service Profile explicitly define a finite output family. Any unexpected difference invalidates the complete case.

Semantic checksums verify logical results and detect non-determinism. They do not by themselves prevent compiler constant folding, dead-work elimination, or loop hoisting, and they are not authentication, artifact identity, or evidence of translation quality.

Every duration Measurement Method declares an optimization-barrier policy:

```text
OptimizationBarrierPolicy {
  input opacity method
  output consumption method
  barrier placement relative to the interval
  semantic validation method
}
```

The method MUST prevent the compiler or runtime from treating the complete measured input as a compile-time constant, hoisting the measured operation across repetitions, or deleting its result as unused. It MAY use a platform benchmark black box, runtime-materialized input, an opaque host boundary, or another owner-verified mechanism. Baseline and candidate use the same policy. Unavoidable barrier work inside the interval is declared and is never removed by estimated subtraction.

Semantic validation remains outside the interval where the owner boundary requires it and checks the same operation and deterministic input sequence. The Optimization Barrier prevents work elimination; the semantic checksum proves the logical observation.

### Warmup, samples, and repetitions

A Measurement Profile records:

- warmup strategy and count;
- measured sample count;
- positive repetitions per sample or a deterministic calibration rule;
- Sample Aggregation Kind;
- process reuse or restart policy;
- fixture reset policy;
- Scratch Workspace, allocator, cache, and output-buffer reset/reuse policy;
- case order or interleaving policy;
- minimum clock-resolution relationship;
- timeout and cancellation behavior; and
- required raw sample retention.

Warmup completes before measured samples and contributes no value to a statistic. Fixture or persistent state is restored to the declared pre-sample state before every warmup and measured sample when the operation mutates state.

Each sample records:

- zero-based ordinal;
- positive repetition count;
- aggregate exact quantity for those repetitions;
- semantic observation identity; and
- optional measurement-method diagnostics.

Revision `"0"` gives the aggregation kinds these meanings:

- `batch_total` — total duration or additive count over the complete repetition batch;
- `peak_over_batch` — maximum observed value over the repetition batch, never divided by repetition count;
- `terminal_value` — value at the declared terminal state; and
- `deterministic_value` — one exact value from one deterministic generation.

Numeric comparison of `batch_total` samples requires the same fixed repetition count on baseline and candidate. An automatic calibration may select that count before measurement, but a gating run freezes and applies the selected count to both sides. Per-operation rational values are presentation-only in revision `"0"`.

Each deterministic generation is retained as an independent sample with `repetitionCount = 1`. A deterministic gate requires at least two generations and identical quantities and semantic or artifact observations. A mismatch is an integrity/determinism failure, not a performance regression.

All duration conversion, counter accumulation, repetition handling, and derived arithmetic use checked operations. `measurement-overflow`, `counter-overflow`, `repetition-overflow`, and `duration-conversion-overflow` are explicit measurement failures; no wrapped or saturated `u64::MAX` sample is emitted.

Revision `"0"` requires raw ordered sample retention for any evidence used by common numeric comparison or a Performance Budget. A local smoke profile may use one measured sample; it remains observational and cannot satisfy a numeric gate.

Automatic outlier deletion is not allowed in revision `"0"`. A contaminated run is rejected or retained visibly. A future profile may add a deterministic exclusion method only with a specification revision and complete raw-sample preservation.

### Measurement environment

Every evidence set records one finite Environment Observation. It includes at least:

```text
RunnerContext =
  local-uncontrolled
  | controlled-unqualified { runner class identity }
  | qualified {
      runner class identity
      Runner Qualification Evidence identity
    }
```

- operating-system family, version, and kernel/runtime build where observable;
- CPU architecture and target triple;
- Runner Context;
- physical or virtualized execution kind;
- processor model/class and available logical CPU count;
- memory capacity class;
- power/thermal policy when the platform exposes a controlled value;
- language runtime, browser, VM, or device model and version, including applicable JIT tier/warmup and garbage-collector configuration;
- compiler/toolchain identity;
- build profile, optimization, debug assertions, link mode, and relevant feature set;
- instrumentation mode, including whether timing, allocation, trace, or other profiling is compiled or enabled;
- allocator and memory-observer identity;
- monotonic-clock or sampler identity and resolution;
- process, worker, thread, and concurrency policy;
- container/emulator/simulator identity where applicable;
- Locale Service Profile and data revision when applicable; and
- measurement harness and projection revisions.

The Environment Observation uses controlled identifiers, not a raw environment dump. Hostname, username, home directory, repository path, arbitrary environment variables, access tokens, and command-line secrets are forbidden.

The complete observation is retained for diagnosis. A Comparison Profile selects which fields form its Environment Class and which may differ. Omitted required fields make the pair not comparable.

`local-uncontrolled` evidence requires no controlled runner-class identity and remains observational. `controlled-unqualified` records a candidate Runner Class Specification but cannot gate. `qualified` additionally references valid Runner Qualification Evidence and is the only Runner Context eligible for duration or memory gating.

### Build and subject identity

The evidence separately records:

- source revision or content identity;
- implementation package/crate/binary revisions;
- exact executable or module digest when available;
- dependency lock identity;
- build configuration;
- Target Profile and physical engine kind;
- generated artifact and Release identities; and
- Measurement Profile identity.

Implementation revision is intentionally allowed to differ between baseline and candidate. Build configuration and all other fields selected by the Comparison Profile must remain compatible. A Profiling Build is not compatible with an ordinary uninstrumented performance baseline unless a specialized observational profile explicitly permits that difference; it cannot produce a gating comparison against that baseline.

A dirty source tree may produce local observational evidence if recorded as such. It cannot become an authorized shared baseline or Release gate unless the product workflow can identify its complete source content.

### Failure, skip, and unsupported measurement

Owner harness execution distinguishes:

- expected semantic checked or blocked result — successful measured operation;
- semantic mismatch or checksum mismatch — invalid benchmark result;
- operational panic/crash/timeout — failed invocation with no successful prefix;
- duration, counter, repetition, or conversion overflow — explicit failed measurement with no numeric sample;
- unsupported physical measurement method — explicit unsupported measurement;
- non-applicable case — only when profile applicability proves it; and
- missing required case — incomplete result.

A required performance gate cannot pass from a skipped, unsupported, failed, incomplete, or projection-ineligible case.

## Comparison Admission

### Comparison Profile

A Comparison Profile is immutable, versioned policy. It declares:

- admitted 026 specification revision;
- admitted owner schema/profile and Measurement Projection revisions;
- selected Measurement Case or finite case group;
- required common category, operation class, metric, and unit;
- equal case dimensions;
- explicitly permitted subject or target differences;
- Environment Class fields and compatibility predicates;
- sampling and pairing requirements;
- Statistic Selection; and
- baseline-selection scope.

There are no implicit defaults. A missing Statistic Selection, sample minimum, or environment rule makes the profile invalid. A finite case group expands into an independent Comparison Evaluation for each selected Measurement Case; it does not average or merge unlike cases. An aggregate comparison requires an aggregate subject to exist as its own Measurement Case.

### Statistic Selection

One immutable Statistic Selection can be referenced by a Comparison Profile or a direct Performance Budget:

```text
StatisticSelection {
  identity and revision
  kind:
    exact | minimum | maximum |
    nearest_rank_p50 | nearest_rank_p95
  minimum sample count
  deterministic-measurement requirement
}
```

Measurement Profiles own collection, Comparison Profiles own compatibility and paired/unpaired comparison, and Performance Budgets own limits, tolerances, and advisory/gating disposition. A Statistic Selection is the shared rule for deriving one scalar without moving those responsibilities between layers.

### Compatibility procedure

Before calculating a statistic, the evaluator checks in this order:

1. both evidence sets and their owner results are admitted and integrity-consistent;
2. both use the exact Measurement Projection admitted by the Comparison Profile;
3. case identity fields required equal by the profile match;
4. any differing target, engine, artifact, or implementation fields are explicitly permitted;
5. semantic observation identities match or satisfy the declared logical equivalence relation;
6. metric, unit, Measurement Method Descriptor, interval meaning, Sample Aggregation Kind, and sampling model are compatible;
7. every required Environment Class field is present and compatible;
8. both evidence sets satisfy sample-count, fixed repetition, fixture-reset, and pairing requirements; and
9. neither input is failed, incomplete, unsupported, or observational-only when a gate is requested.

The first failure is retained together with all other safely detectable compatibility reasons. No numeric statistic is emitted for an incompatible pair.

### Comparison modes

Revision `"0"` defines three numeric comparison modes:

| Mode | Use |
| --- | --- |
| `same_environment_regression` | Compare implementation revisions for the same case on one compatible controlled runner class |
| `paired_implementation` | Interleave baseline and candidate on the same runner to reduce drift while preserving separate raw samples |
| `paired_target_path` | Compare runtime-backed, ahead-of-time, or platform-native paths when one profile explicitly names allowed target/path differences and semantic equivalence |

### Cross-platform descriptive reporting

Cross-platform side-by-side presentation is not a Comparison Profile mode. A Cross-Platform Report Profile selects evidence rows, grouping, ordering, required context, and exact displayed quantities. It has no Baseline, Statistic Selection, tolerance, difference, ratio, or numeric pass/fail result.

Such a report may contain unlike environments and targets, but each incompatible row is visibly labeled descriptive and non-comparable. A numeric cross-target experiment instead uses `paired_target_path` under one explicit Comparison Profile.

### Statistics

Deterministic artifact-size and delivery-topology cases use one exact quantity after at least two independently retained generation samples prove quantity and semantic-output equality. Each sample has `repetitionCount = 1`. A Comparison Profile may admit another count metric as deterministic only when its Measurement Method Descriptor requires the same exact value across generations.

Duration, memory, and non-deterministic sampled count comparisons select one of the following revision-`"0"` statistics:

- `minimum`;
- `maximum`;
- `nearest_rank_p50`; or
- `nearest_rank_p95`.

Nearest-rank percentile for `p` over `N` ordered quantities selects the one-based element at `ceil(p × N)`. No interpolation or floating point is used. `p50` and `p95` therefore have identical results across implementations for the same sample vector.

A profile must justify `minimum` if used for gating because it emphasizes ideal rather than typical behavior. Runtime hot-format budgets normally use `nearest_rank_p50` and may additionally report `nearest_rank_p95`. Peak-memory budgets normally use `maximum`. These are guidance, not hidden defaults.

For a sample that aggregates several repetitions, the statistic operates on the aggregate sample quantity. Per-operation display divides by the exact repetition count as a rational value; it does not replace the stored sample.

### Paired comparison

`paired_implementation` and `paired_target_path` record an explicit ordered run schedule and pair identity. Each logical pair contains exactly one baseline sample and one candidate sample with the same repetition count, Environment Observation, declared fixture reset, and Execution State. A profile may choose `AB` or `BA`; a fixed balanced block such as `ABBA` represents two explicitly identified pairs rather than one four-observation pair. The exact sequence and pair assignment are profile data.

The evaluator creates baseline and candidate vectors from complete pairs, applies the same Statistic Selection independently to both vectors, and derives the signed difference and exact ratio from those two scalar statistics. Pairing controls acquisition order and incomplete-sample handling in revision `"0"`; the evaluator does not apply a percentile to a vector of pair differences. A future difference-distribution method requires a later specification revision. Samples are joined only by explicit pair identity, never timestamp or array position.

An interrupted pair is incomplete and contributes neither side to a gating statistic. Its partial observations remain available for diagnosis but cannot be re-paired with another run.

### Cross-target interpretation

Common units make reports consistent, not automatically comparable.

Valid numeric cross-target experiments include:

- Runtime-backed and ahead-of-time output on the same browser and device;
- two physical MF2 engines built for the same native target and runner class;
- complete and pruned artifacts generated from the same checked Target Profile inputs; and
- Browser and SSR paths measured separately on their own stable runner classes and compared only to their own baselines.

Examples that are descriptive unless a specialized profile is admitted include:

- browser hot formatting on a laptop versus Swift formatting on a phone;
- simulator versus physical device;
- Node.js N-API versus browser WASM on different machines; and
- artifact sizes from semantically different Target Profiles.

026 never produces one Intlify-wide performance score.

## Baselines, Regressions, and Budgets

### Baseline lifecycle

A baseline is an explicit immutable pointer to admitted Measurement Evidence, not an implicit lookup of the latest successful run on `main`.

A Baseline Selection records:

- stable baseline-scope ID;
- Comparison Profile identity;
- Measurement Case identity;
- selected evidence and subject implementation identity;
- selecting actor and authority when shared or Release-gating;
- selection reason;
- superseded baseline identity when applicable; and
- optional expiry or review condition.

Selection and supersession create new records. They do not mutate evidence or delete historical baselines.

A candidate cannot select itself as its baseline. A failed candidate cannot automatically refresh the baseline. Scheduled refresh, dependency upgrades, compiler upgrades, runner replacement, or intentional architecture changes require an explicit new selection and a reviewable discontinuity.

When an Environment Class or Measurement Profile changes, the old baseline remains historical but is not comparable. The new class starts unbaselined unless a controlled bridging campaign records both classes; a bridge is report context and cannot mathematically normalize unrelated future runs.

### Direct and relative budgets

A Performance Budget is supplied by the applicable target, Runtime, product, or release design. 026 defines its evaluation shape.

A direct budget references one candidate Measurement Evidence Set, Measurement Case, and Statistic Selection. A baseline-relative budget references one Comparison Evaluation whose Comparison Profile has already derived compatible baseline and candidate statistics. The budget owns its exact limits and tolerances plus the `report`, `warn`, or `block` workflow disposition for non-passing and unavailable outcomes.

Revision `"0"` admits:

- `maximum` — candidate statistic must be less than or equal to an exact quantity;
- `minimum` — candidate statistic must be greater than or equal to an exact quantity;
- `range` — candidate statistic must fall within inclusive exact lower and upper quantities;
- `exact` — candidate quantity must equal an exact deterministic value; and
- `baseline_relative_maximum` — candidate regression over a selected baseline must remain within exact absolute and relative tolerance.

Duration, size, allocation, materialization, boundary-call, delivery-topology, and memory costs normally use upper bounds. A minimum is available for metrics where larger is intentionally better, but revision `"0"` defines no canonical throughput metric.

Performance Budgets are distinct from Resource Limit Policies:

| Resource limit | Performance budget |
| --- | --- |
| Bounds admitted input or execution work semantically | Evaluates observed implementation cost |
| Failure is part of normative operation behavior | Failure is a CI/Release/product quality decision |
| Must hold on every execution | Holds under one admitted measurement case/environment |
| Often protects security or availability | Prevents footprint or performance regression |

A performance result never permits a Resource Limit Policy violation.

### Relative tolerance calculation

For `baseline_relative_maximum`, the budget contains:

- `absoluteTolerance` in the metric's canonical unit; and
- `relativeTolerancePpm` as an integer parts-per-million value.

The allowed increase is:

```text
relative = ceil(baseline × relativeTolerancePpm / 1_000_000)
allowedIncrease = max(absoluteTolerance, relative)
limit = baseline + allowedIncrease
```

All arithmetic is checked with a wider intermediate representation. Overflow makes the budget invalid; it is never saturated silently.

The candidate passes when `candidate <= limit`. The evaluation retains the exact baseline, candidate, absolute tolerance, relative tolerance, computed increase, and limit.

Using the maximum of absolute and relative tolerance prevents noise near zero from making a small absolute change look catastrophic while preserving proportional control for larger values. A target may set either tolerance to zero explicitly.

### Evaluation outcome

Every evaluation uses one closed outcome:

- `pass` — compatible evidence satisfies the budget;
- `warn` — a policy-designated advisory budget is exceeded;
- `fail` — a gating budget is exceeded;
- `not-comparable` — evidence exists but compatibility admission failed;
- `unbaselined` — a relative evaluation has no applicable selected baseline; or
- `incomplete` — required valid evidence could not be produced because it was missing, skipped, unsupported, failed, or projection-ineligible; or
- `invalid-evidence` — evidence, projection, baseline, or budget failed admission.

Typed `incomplete` reasons include `missing-evidence`, `missing-required-case`, `skipped-required-case`, `unsupported-measurement`, `failed-invocation`, `projection-ineligible`, and `runner-not-qualified`. An unavailable input is not mislabeled invalid merely because it cannot satisfy a gate.

Outcome and workflow disposition remain separate. A Performance Budget or product workflow decides whether `not-comparable`, `unbaselined`, or `incomplete` is reported, warned, or blocked. Release-gating policy MUST block all three; a local observational workflow may report them without failing.

### Budget ownership and aggregation

Target designs own their numeric budgets and applicable cases. A Deployment Compatibility Group may add a group-level budget for its complete target outputs or hydration-critical path. 026 owns evaluation but does not invent those numbers.

A group budget never averages away a failing required member budget. Aggregation is allowed only when the budget itself defines an exact aggregate subject such as:

- complete Release bytes;
- total initial/eager-load bytes;
- all required locale artifacts;
- a fixed hydration path; or
- an explicit peak-memory scenario.

Per-locale, per-delivery-unit, per-kind, and per-engine observations remain individually available.

## Reporting

### Structured report

A structured report contains:

- report specification revision;
- exact selected evidence, Comparison Profile or Cross-Platform Report Profile, Statistic Selection, baseline, and budget identities;
- conformance and capability summaries;
- measurement rows with owner and common names;
- performance surface, reuse state, transfer representation, and delivery-topology context;
- Environment Class and relevant differing fields;
- raw-sample references and selected statistic;
- exact quantities, differences, and ratios;
- compatibility and evaluation outcomes with typed reasons;
- missing/unsupported case inventory; and
- truncation state.

A report is a projection of evidence and decisions. It does not become a second authority for samples or semantic results.

Machine consumers use structured fields. They do not scrape human Markdown, terminal tables, chart labels, or CI log text.

### Human report

A human report should lead with:

- semantic conformance and capability status;
- blocking budget or compatibility outcomes;
- changed owner phase/cost;
- workload and execution state;
- performance surface and whether host materialization or workflow overhead is included;
- baseline and candidate statistic;
- runner/environment class; and
- whether the result is gating, advisory, or observational.

It may group rows under common categories and show ms, µs, KiB, MiB, percentages, sparklines, or charts. It always retains a route to the exact structured quantities and evidence identities.

Profiler output is presented in a separate diagnostic view labeled with its Profiling Build and instrumentation capabilities. It is not placed in the same numeric comparison column as uninstrumented benchmark evidence.

Cross-Platform Report Profiles clearly label side-by-side values as descriptive and non-comparable; they do not emit a Comparison Evaluation.

### Finding projection

026 evaluation failures may be projected as 019 Findings. Stable codes and exact code allocation belong to the implementation specification, but categories include:

- invalid owner result or projection;
- missing required conformance case;
- failed capability evidence;
- semantic checksum mismatch;
- unsupported measurement method;
- incomplete measurement evidence;
- runner qualification or preflight failure;
- instrumentation-isolation failure;
- incompatible environment or case;
- missing baseline;
- exceeded advisory or gating budget; and
- incomplete hydration render-equivalence evidence.

A measurement Finding references evidence and policy identities. It does not copy an approximate humanized value as the only diagnostic data.

## CI and Release Policy

### Verification levels

026 defines four workflow levels:

| Level | Required behavior |
| --- | --- |
| `integrity` | Validate suite/result schemas, required cases, registry and interval topology, fixture identities, checksums, deterministic outputs, projection mappings, and report generation |
| `observational` | Collect physical samples and publish trends without a numeric pass/fail decision |
| `advisory` | Compare against an admitted baseline/budget and report warnings without blocking the ordinary change |
| `gating` | Apply an explicitly approved stable-runner policy and block the configured CI or Release decision on failure, missing evidence, or incompatibility |

Every implementation begins with `integrity`. It may add `observational` immediately. `advisory` and `gating` require baseline and runner lifecycle operations.

### Normal pull-request CI

Normal CI always gates:

- deterministic conformance fixtures selected for the changed closure;
- benchmark harness compilation and smoke execution;
- owner result-schema and required-case validation;
- interval boundary and overlap tests;
- semantic checksum stability;
- exact quantity/unit validation;
- Measurement Projection tests; and
- common structured report generation.

Where an implementation provides profiling, normal CI also compiles and smoke-tests the profiling feature separately, verifies enabled/disabled logical-result equivalence, and validates Instrumentation Isolation Evidence proving that ordinary production feature selection does not retain profiler call-site work, runtime, or recorder dependencies.

Deterministic artifact-size and delivery-topology budgets may also gate normal CI when at least two retained generations of identical checked inputs produce byte-identical outputs and exact topology counts.

Machine-sensitive duration and memory values are observational until a Runner Class Specification, valid Runner Qualification Evidence, and Comparison Profile are explicitly promoted.

### Stable performance CI

A stable runner is defined by an immutable Runner Class Specification:

```text
RunnerClassSpecification {
  identity and revision
  required environment predicates
  required control settings
  qualification workload
  noise and drift statistic
  acceptance thresholds
  preflight checks and cadence
  expiry condition
  invalidation triggers
}
```

Qualification produces:

```text
RunnerQualificationEvidence {
  runner class identity
  privacy-safe runner instance identity
  Environment Observation
  ordered qualification samples
  selected statistic
  outcome: qualified | unqualified | incomplete | invalid
  typed reasons
  validity condition
}
```

The platform or product owner supplies the concrete thresholds; 026 owns this shape and evaluation behavior. A qualified runner has:

- a versioned runner-class identity;
- pinned hardware or device class;
- controlled OS/runtime/toolchain/build revisions;
- controlled CPU governor, power, thermal, and background-load policy where applicable;
- exclusive or declared contention behavior;
- calibrated monotonic clock or memory observer;
- retained Environment Observations;
- periodic noise and drift checks; and
- an explicit baseline refresh process.

Every gating run performs the class-defined preflight and proves that qualification remains valid, Environment Class fields still match, observers remain usable, thermal/power/background-load controls remain in policy, and the noise probe remains within threshold. Qualification expires or is invalidated after an applicable hardware, OS, runtime, toolchain, allocator, clock, power-policy, or Runner Class Specification change; threshold failure; or missed required check cadence.

Gating duration and memory benchmarks run with diagnostic profiling disabled unless their Measurement Profile defines the instrumentation itself as the measured subject. A separate Profiling Build may accompany a regression for diagnosis but cannot replace the uninstrumented evidence.

A run that fails preflight remains visible as observational Measurement Evidence but cannot make a gating decision. Its evaluation is `incomplete` with `runner-not-qualified`; it is not deleted or mislabeled as a budget regression.

Mobile physical-device farms and browser runners may use target-specific stability checks. Simulator/emulator evidence remains a distinct Environment Class.

### Release evidence

025 may require exact Conformance Evidence, Capability Evidence, an `equivalent` Logical Render Equivalence Evaluation, and Budget Evidence before Release publication or deployment activation. Such evidence is admissible only when:

- its subject identities match the Release and Target output identities;
- every applicable suite and policy revision is admitted;
- it is complete and not stale;
- the producing actor and runner evidence satisfy 018-owned trust policy; and
- the publication policy explicitly names the required evidence set.

Local developer evidence is not automatically Release authority.

Production Release artifacts do not include profiling instrumentation, recorder state, or profiling-only dependencies unless a Target Profile explicitly defines a separate diagnostic product. Such a diagnostic product has a distinct build and artifact identity and cannot be substituted for the ordinary Release artifact.

## Security, Privacy, and Trust

Benchmark fixtures, localized artifacts, and Runtime inputs are untrusted data unless admitted by their owning specifications. Harnesses apply the same resource limits and sandboxing expectations as the operations they invoke.

Measurement collection:

- does not gain Provider, TMS, governance, publication, or deployment credentials;
- does not run arbitrary code embedded in translated messages;
- does not upload source, messages, artifacts, traces, or machine details implicitly;
- redacts or omits application content when a shared report needs only digests and logical counts;
- uses controlled environment fields rather than raw environment dumps;
- bounds samples, trace bytes, diagnostics, and retained evidence; and
- treats profiler and crash outputs as potentially sensitive separate attachments.

An evidence digest proves content identity, not who ran it or whether the runner was controlled. 018-owned signatures, attestations, and actor policy determine whether shared or Release-gating evidence is trusted.

## Initial 015 Adoption

026 is designed so the minimum 015 resolver implementation can adopt the stable measurement shape before cross-target Runtime work exists.

### Initial profile

The initial 015 implementation uses an owner-defined profile conceptually named `015-profile-resolver-observational-v0`. The final file, module, and public label remain implementation choices.

It has these properties:

- every active 015 phase/cost keeps its exact 015 name and boundary;
- ordinary component and end-to-end duration cases map to `toolchain`;
- `profile_resolve_peak_memory` maps to `memory`;
- durations are recorded as losslessly retained canonical nanoseconds under one declared clock/conversion Measurement Method Descriptor;
- supported memory and allocation observations use defined common octet/count metrics and complete method/domain descriptors; total allocated bytes remains owner-specific in revision `"0"`;
- ordered raw samples and positive repetition counts use the common semantics;
- each sample carries the 015 semantic checksum observation;
- the full 015 logical work vector is retained;
- the owner result records complete build and Environment Observation inputs needed by its projection;
- CI gates registry, required-case, interval, checksum, deterministic-output, projection, and report integrity; and
- no revision-`"0"` duration or memory value is a numeric CI budget.

The initial implementation may also expose non-default timing spans aligned with the 015 phase/cost boundaries and a separately enabled allocation observer. Those Profiler Observations support local diagnosis only. Projection-ready benchmark samples are collected with diagnostic profiling disabled, so adding the profiler does not redefine the 015 Measurement Cases.

### Minimum projection mappings

| 015 owner pair                       | Common category | Operation class | Performance Surface |
| ------------------------------------ | --------------- | --------------- | ------------------- |
| `profile_resolver_construct` costs   | `toolchain`     | `component`     | `core`              |
| `profile_entry_materialize` costs    | `toolchain`     | `component`     | `core`              |
| `profile_structural_admit` costs     | `toolchain`     | `component`     | `core`              |
| `profile_select` costs               | `toolchain`     | `component`     | `core`              |
| `profile_locale_resolve` costs       | `toolchain`     | `component`     | `core`              |
| `profile_artifact_admit` costs       | `toolchain`     | `component`     | `core`              |
| `profile_target_group_resolve` costs | `toolchain`     | `component`     | `core`              |
| `profile_evidence_materialize` costs | `toolchain`     | `component`     | `core`              |
| `profile_resolve_e2e` costs          | `toolchain`     | `workflow`      | `product_workflow`  |
| `profile_resolve_peak_memory` costs  | `memory`        | `peak_live`     | `core`              |

This table does not define the owner pair meanings. It only fixes their common grouping. Each `profile_resolve_peak_memory` case additionally references the 015-owned Memory Observation Domain for resolver construction or invocation.

### Avoided reimplementation

The first 015 harness must not emit only a console average. It retains:

- exact owner schema/profile revision;
- fixture and workload identity;
- phase/cost and interval identity;
- exact raw quantities and sample/repetition structure;
- semantic checksum;
- build and controlled environment metadata;
- metric provider identity; and
- a versioned mapping to the common category.

With those fields, later work can add an immutable baseline and Comparison Profile without moving timers, changing semantic checksums, or replacing the product-owned result schema. A future schema revision may add data, but numeric policy does not require a second resolver benchmark architecture.

015 ResourceBoundValue and Resource Limit Policy cases remain conformance inputs. They are never derived from the physical measurements above.

## Conformance and Measurement Fixtures

### Common fixture families

026 owns fixtures for:

1. Measurement Projection admission and rejection;
2. exact numeric/unit and duration-conversion behavior;
3. Measurement Method Descriptor and Memory Observation Domain admission;
4. Environment Class and Runner Context compatibility;
5. Sample Aggregation Kind, repetition, and Statistic Selection derivation;
6. baseline and budget lifecycle;
7. Capability Declaration coverage;
8. Finding projection preservation;
9. logical execution equivalence;
10. Browser/SSR hydration render equivalence;
11. profiler feature isolation, hierarchy, completion, and bounded recording;
12. performance-surface, Artifact Set Scope, and delivery-topology identity;
13. Runner Qualification Evidence; and
14. comparison versus descriptive-report separation and report determinism.

Component specifications own the semantic fixture bodies imported by those campaigns.

### Required projection fixtures

Every Measurement Projection includes cases for:

- the exact admitted owner schema/profile revision;
- unknown phase or cost;
- missing required owner record;
- duplicate owner record;
- wrong metric or unit;
- missing or unknown Performance Surface;
- missing or unknown Artifact Set Scope when required;
- missing or incompatible Memory Observation Domain when required;
- missing or incomplete Measurement Method Descriptor;
- missing or unknown Sample Aggregation Kind;
- inconsistent Execution State;
- exact zero and `u64::MAX` quantity;
- first-over or lossy numeric input rejection;
- exact and rounded duration conversion plus NaN, infinity, reversal, and overflow rejection;
- absent required raw samples;
- checksum mismatch;
- interval-boundary mismatch;
- missing environment field;
- unsupported measurement method;
- owner failure or partial prefix;
- allowed optional owner metadata; and
- deterministic output under input-order permutation where the owner permits permutation.

### Required profiling fixtures

Profiling fixtures cover:

- an ordinary build with instrumentation call sites disabled and no required profiler recorder dependency;
- Instrumentation Isolation Evidence covering dependency closure, symbols/imports/registries/sections, representative code generation, and non-evaluation of disabled arguments;
- enabled/disabled logical-result and deterministic-artifact equality;
- event-trace and aggregate-table recorder modes;
- nested sibling and repeated spans with correct complete occurrence, inclusive-total, and self-total relationships;
- cancelled, unwound, truncated, and recorder-failed span completion;
- separate worker roots when no cross-context parent is declared;
- explicit propagated context when asynchronous parentage is supported;
- static registry rejection of an unknown span ID;
- record-count, depth, retained-byte, and report-size exact-boundary and first-over behavior;
- explicit truncation propagation;
- allocation observation with declared included and excluded domains;
- counter overflow or observer failure without silent saturation;
- absence of source content and dynamic user values from ordinary shared span labels; and
- rejection when a Profiler Observation is submitted directly as a benchmark owner result.

### Required comparison fixtures

Comparison fixtures cover:

- identical evidence;
- permitted implementation revision difference;
- each incompatible case dimension;
- each missing Environment Class field;
- equal and unequal semantic observations;
- one-sample observational ineligibility;
- exact percentile selection for odd and even sample counts;
- fixed versus mismatched repetition counts for each Sample Aggregation Kind;
- `AB`, `BA`, and explicitly two-pair `ABBA` behavior;
- paired independent baseline/candidate Statistic Selection and interrupted-pair behavior;
- baseline zero;
- candidate larger, equal, and smaller;
- tolerance exact boundary and first-over;
- checked-arithmetic overflow;
- missing and superseded baseline;
- deterministic size evidence;
- incompatible memory providers;
- core-only versus boundary-inclusive surface mismatch;
- profiling versus uninstrumented build mismatch;
- transfer-representation mismatch without an explicitly paired profile;
- generated-file, Delivery Unit, initial-load-request, and complete-load-request exact counts;
- `incomplete` outcomes for each typed missing/unsupported/failed/projection/runner reason;
- Cross-Platform Report Profile output with no Comparison Evaluation, ratio, or budget result; and
- stable report ordering.

### Required runner-qualification fixtures

Runner fixtures cover:

- local-uncontrolled, controlled-unqualified, and qualified Runner Contexts;
- exact qualification threshold and first-over failure;
- valid and expired qualification;
- each environment-change invalidation trigger;
- missed check cadence;
- preflight noise, thermal, power, background-load, clock, and memory-observer failure; and
- retention of failed-preflight observations with `incomplete / runner-not-qualified` evaluation.

### Required logical-equivalence fixtures

Logical-equivalence fixtures cover:

- exact text equality;
- exact structured-parts equality;
- changed part order or nesting;
- changed requested or definition locale;
- changed selected artifact;
- dropped or changed diagnostic;
- pinned locale-service equality;
- each admitted platform-managed variation;
- variation outside the declared set;
- Runtime-backed versus ahead-of-time execution; and
- Browser/SSR hydration equality and mismatch.

## Candidate Implementation Boundaries

Exact crate, package, and command names are not frozen. A reusable implementation is expected to separate:

```text
owner harnesses
  -> checked owner result
  -> measurement projection registry
  -> common evidence admission
  -> comparison and budget evaluator
  -> structured report projection

component suites
  -> conformance campaign runner/importer
  -> logical observation codecs
  -> capability and equivalence evidence
```

Candidate internal components are:

- a language-neutral evidence model and validator;
- a registry of versioned owner Measurement Projections;
- exact quantity/statistic helpers;
- Measurement Method Descriptor, Memory Observation Domain, Execution State, and Statistic Selection validators;
- optional static span registry, bounded profiler recorder, and diagnostic report projection;
- Instrumentation Isolation Evidence adapters for supported build systems;
- a comparison and budget evaluator;
- Runner Class Specification and qualification/preflight evaluator;
- Cross-Platform Report Profile projection;
- a conformance campaign planner and evidence validator;
- shared fixture codecs;
- structured JSON or binary report projection; and
- product adapters for Rust, Node.js, browser, mobile, and native runners.

Production libraries and target artifacts do not depend on benchmark runners, sample collectors, comparison history, or report renderers.

Instrumentation is test/benchmark-only or guarded behind non-default implementation features. Instrumentation Isolation Evidence proves that disabled call sites compile without a required runtime branch, atomic operation, thread-local access, argument evaluation, allocation, or recorder dependency. Instrumentation cannot change normal product artifact formats or ship in a release merely because a benchmark uses an optimized build profile.

The profiler recorder and benchmark sample collector remain separate components. They may share clocks or allocation-observer adapters, but a profiler report cannot be passed directly to evidence admission as though it were an owner benchmark result.

## Implementation Phasing

Implementation phases are dependency-ordered capability slices, not Runtime phases, Roadmap milestones, PR boundaries, or a promise that all targets land together.

### Phase 1 — Common measurement foundation and 015 adoption

- Define revision-`"0"` performance surfaces, Memory Observation Domains, Artifact Set Scopes, categories, metrics, Measurement Method Descriptors, Execution State, Sample Aggregation Kinds, exact quantities, duration conversion, Environment Observation, Runner Context, and projection validation.
- Implement Statistic Selection, exact difference/ratio, fixed-repetition, overflow, and compatibility primitives.
- Define a compile-time-disabled span facade and bounded optional hierarchical timing recorder; keep allocation observation a separately enabled capability.
- Add the initial 015 Measurement Projection and projection fixtures.
- Make 015 benchmark smoke results retain projection-ready raw samples, checksum, workload, reuse state, build, and environment data.
- Add Instrumentation Isolation Evidence and feature-matrix tests for profiling-disabled ordinary builds and profiling-enabled semantic equivalence.
- Gate integrity and deterministic behavior; keep physical values observational.

Phase 1 is complete when one 015 result can be validated, projected, reported, and rejected by every applicable negative fixture without changing any 015 semantic operation boundary, and profiling can be enabled for diagnosis without changing the ordinary build's logical result or required runtime path.

### Phase 2 — Baseline, comparison, and budget evaluation

- Implement immutable Baseline Selection.
- Implement all three revision-`"0"` numeric Comparison Profile modes, Statistic Selection, and Cross-Platform Report Profile projection.
- Implement exact direct and baseline-relative budget evaluation.
- Implement Runner Class Specification, qualification evidence, preflight input, invalidation, and structured reasons.
- Establish an advisory resolver baseline on a controlled runner without making it a revision-`"0"` normal-CI gate.

Phase 2 is complete when repeated evaluation over the same evidence and policies is deterministic and all tolerance, incompatibility, overflow, and baseline-lifecycle fixtures pass.

### Phase 3 — Common conformance campaign foundation

- Integrate 017 artifact/version admission and 019 Finding projection.
- Define suite/campaign, Conformance Campaign Evaluation, tagged applicability, Logical Result, Capability Evidence, and Logical Render Equivalence Evaluation implementations.
- Import component-owned suites without copying their semantic authority.
- Add complete/incomplete/invalid campaign behavior.

Phase 3 is complete when one multi-suite campaign proves capability coverage and Finding preservation with no implicit skip or directory-discovered authority.

### Phase 4 — Execution, Web, and reference Runtime evidence

- Integrate 023–025 logical execution, target, and Release identities.
- Add reference Runtime initialization, loading, preparation, cold/hot formatting, parts, cache, runtime-compilation/managed-heap states, artifact-size, delivery-topology, boundary, and memory profiles.
- Compare core-only and host-materialized paths and retain transfer representation, boundary call, and object-materialization observations.
- Establish Web baseline evidence for the I1 vertical slice.
- Add Runtime-backed versus ahead-of-time equivalence and measurement reports.

Phase 4 is complete when 027/028 can prove semantic conformance and emit the I1 footprint baseline required by 000 without conflating owner phases or environment classes.

### Phase 5 — Cross-target and Release evidence

- Add Browser/SSR hydration equivalence campaigns.
- Add iOS, Android, and native runner/environment profiles.
- Add paired target-path comparisons where physically meaningful.
- Add target-owned budgets and group-level Release evidence.
- Keep unrelated platforms descriptive rather than synthetically normalized.

Phase 5 is complete when Web, mobile, and native implementations can use the same evidence semantics, each claimed capability is covered, and every numeric comparison is backed by an admitted Comparison Profile.

## Validation Strategy

The implementation validates:

- schema and version admission;
- complete content-addressed suite closure;
- owner authority preservation;
- projection losslessness;
- exact unit and numeric handling across Rust and JavaScript;
- deterministic duration conversion and checked measurement overflow;
- interval and overlap topology;
- semantic checksum stability;
- Optimization Barrier behavior independent from semantic checksum validation;
- Sample Aggregation Kind, fixed repetition, deterministic-generation, and Statistic Selection behavior;
- performance-surface identity and core/boundary separation;
- Memory Observation Domain, Artifact Set Scope, and Execution State identity;
- deterministic generated-file, Delivery Unit, initial-load-request, and complete-load-request counting;
- profiling enabled/disabled logical equivalence;
- Instrumentation Isolation Evidence and absence of profiler runtime work/dependencies from ordinary product feature selection;
- profiler recorder mode, span completion, aggregate total, self/inclusive-time, context, bound, and truncation behavior;
- allocation-profiler domain and self-observation disclosure;
- rejection of profiler output presented directly as benchmark evidence;
- comparison compatibility;
- numeric comparison versus Cross-Platform Report Profile separation;
- Runner Qualification Evidence, expiry, invalidation, and preflight behavior;
- baseline and policy immutability;
- budget arithmetic;
- Finding projection;
- capability coverage;
- logical execution and hydration equivalence;
- deterministic report ordering;
- bounded evidence size; and
- absence of benchmark-only code and data from ordinary product artifacts.

Shared vectors are required for exact quantity parsing, percentile selection, differences, ratios, tolerance calculation, compatibility reasons, and evidence/report ordering in every promoted language binding.

## Decision Log

| ID | Decision | Status | Reason |
| --- | --- | --- | --- |
| 026-001 | Keep owner conformance and benchmark result schemas authoritative; use versioned common projections rather than one universal runner/schema | Accepted | Product operations have different semantic phases and payloads, and 001 already fixes product ownership |
| 026-002 | Separate semantic Conformance Evidence from physical Measurement Evidence | Accepted | Machine-dependent values cannot define semantic correctness |
| 026-003 | Preserve owner phase/cost and interval identity in every common observation | Accepted | Grouping must not rename, merge, or reinterpret measured work |
| 026-004 | Use exact integer nanoseconds, octets, and counts; derive floats and human units only for presentation | Accepted | Cross-language evidence and budget arithmetic must be lossless |
| 026-005 | Retain ordered raw samples for any common numeric comparison or budget | Accepted | Aggregate-only reports prevent deterministic statistic changes and noise diagnosis |
| 026-006 | Require an explicit Comparison Profile and Environment Class before numeric comparison | Accepted | Matching units alone do not make physical observations comparable |
| 026-007 | Do not create a cross-platform normalized performance score | Accepted | Hardware and runtime differences cannot be corrected safely by a generic factor |
| 026-008 | Make baselines immutable explicit selections rather than implicit latest-main results | Accepted | Comparison history and intentional discontinuities must remain reviewable |
| 026-009 | Keep target budget values target-owned while standardizing evaluation and evidence | Accepted | 026 can compare consistently without guessing acceptable product costs |
| 026-010 | Distinguish Performance Budgets from normative Resource Limit Policies | Accepted | One evaluates physical quality; the other bounds semantic work and safety |
| 026-011 | Forbid automatic outlier deletion in revision `"0"` | Accepted | Hidden sample removal makes small initial datasets difficult to audit |
| 026-012 | Treat missing, skipped, unsupported, stale, or incompatible evidence as non-passing for a gate | Accepted | Absence of evidence cannot prove performance or conformance |
| 026-013 | Require capability claims to reference complete applicable passing case groups | Accepted | Declared support must be independently testable and cannot self-certify |
| 026-014 | Compare canonical logical results, not private target representation | Accepted | Conforming physical engines may use different code, artifacts, or native resources |
| 026-015 | Require exact logical equality for Browser/SSR hydration relations | Accepted | Platform variation cannot justify a client/server initial-render mismatch |
| 026-016 | Adopt a projection-ready observational profile for the first 015 implementation | Accepted | Capturing stable data now prevents later timer, checksum, and schema rework |
| 026-017 | Allow deterministic artifact-size budgets in normal CI but require promoted stable runners for duration/memory gates | Accepted | Byte output can be reproducible across runners while physical timing and memory are environment-sensitive |
| 026-018 | Remove duplicate and optional work before applying instruction-level specialization | Accepted | Faster primitives do not compensate for avoidable parsing, normalization, materialization, or boundary crossings |
| 026-019 | Classify memory by lifetime and prefer component-owned reusable scratch before requiring arena allocation | Accepted | Immutable artifacts, invocation scratch, worker scratch, caches, and returned output have different ownership and reset needs |
| 026-020 | Keep optional diagnostics, parts, source evidence, traces, and metadata off paths that do not request them | Accepted | Common successful operations should not pay allocation and materialization cost for unused outputs |
| 026-021 | Separate feature-gated profiling, uninstrumented benchmarking, and regression enforcement | Accepted | Diagnostic instrumentation locates cost but perturbs the values that compatible benchmark evidence must quantify |
| 026-022 | Treat host boundaries and generated-output delivery as first-class performance surfaces | Accepted | Core throughput alone does not reveal serialization, materialization, transfer, file, Delivery Unit, or request cost |
| 026-023 | Compare transfer representations end to end rather than assuming binary or native paths are faster | Accepted | Copies and host materialization can dominate compact core representations |
| 026-024 | Require a safe reference path and measured evidence before component-owned SIMD or unsafe specialization | Accepted | Low-level specialization is narrow, platform-sensitive, and unnecessary for most semantic processing |
| 026-025 | Preserve owner-specific diagnostic metrics until their common meaning is stable enough for a versioned promotion | Accepted | Premature common naming would create lossy comparison across unlike observation domains |
| 026-026 | Represent applicable and non-applicable conformance case results as a tagged union | Accepted | A non-applicable case has an exclusion rule and no pass/fail outcome |
| 026-027 | Define semantic Campaign Evaluation, Capability Evidence, and Logical Render Equivalence Evaluation records in 026 | Accepted | 017 can encode these records only after 026 fixes their meaning and required relationships |
| 026-028 | Add `complete_load_request_count` alongside initial-load topology | Accepted | A declared complete-load operation class requires a corresponding deterministic metric and fixture |
| 026-029 | Keep descriptive cross-platform reports outside numeric Comparison Profile modes | Accepted | A side-by-side report has no baseline, statistic, tolerance, ratio, or numeric pass/fail claim |
| 026-030 | Apply the same statistic independently to paired baseline and candidate vectors in revision `"0"` | Accepted | Explicit pairing can reduce acquisition drift without introducing an underspecified statistic over signed differences |
| 026-031 | Add `incomplete` with typed missing, unsupported, failed, projection, and runner reasons to Evaluation Outcome | Accepted | Unavailable valid evidence is neither an incompatible pair nor malformed evidence |
| 026-032 | Separate collection, statistic selection, comparison, and budget/disposition responsibilities | Accepted | Direct and baseline-relative policies need reproducible scalars without overlapping profile authority |
| 026-033 | Separate Memory Observation Domain from Performance Surface | Accepted | A memory measurement must preserve both its execution boundary and its included storage/runtime domain |
| 026-034 | Separate artifact representation stage from Artifact Set Scope | Accepted | Initial/eager and complete closures are set scopes, not representation operations |
| 026-035 | Permit declared deterministic rounding into canonical nanoseconds and retain clock resolution/conversion identity | Accepted | Browser and platform clocks are quantized even though stored evidence quantities must remain lossless |
| 026-036 | Declare Sample Aggregation Kind, require fixed comparable repetition, and fail checked overflow | Accepted | Aggregate totals, peaks, terminal values, and deterministic values cannot share implicit repetition arithmetic |
| 026-037 | Use an explicit Optimization Barrier policy instead of assigning work-elimination prevention to semantic checksums | Accepted | Output validation alone cannot prevent constant folding, hoisting, or dead-work removal |
| 026-038 | Make Profiler Observation permanently diagnostic and distinguish recorder mode, aggregate totals, and incomplete span states | Accepted | Instrumented diagnostic records have different perturbation and completion semantics from benchmark evidence |
| 026-039 | Require Instrumentation Isolation Evidence for a zero-required-runtime-work profiling claim | Accepted | Logical equivalence and dependency checks alone do not prove call-site erasure |
| 026-040 | Bind owner-dependent common counts to complete versioned Measurement Method Descriptors | Accepted | Allocation, boundary, and materialization counts are comparable only under explicit equivalent taxonomies and domains |
| 026-041 | Require Runner Qualification Evidence and per-run preflight for duration or memory gates | Accepted | Controlled-runner labels alone do not prove current noise, drift, observer, or environment validity |
| 026-042 | Separate process, engine, preparation, cache, runtime-compilation, managed-heap, scratch, and output reuse state | Accepted | A hot prepared-message cache does not imply a warm JIT, reused process, or equivalent GC state |
| 026-043 | Distinguish normative requirements, evidence-backed recommendations, optional choices, and guidance | Accepted | Implementers need to know which statements affect conformance and which permit justified alternatives |

## Deferred Follow-Up Notes

The following remain in their owning designs:

- shared wire schemas, canonical encoding, digest framing, version migration, and artifact admission: 017;
- evidence authentication, attestations, runner trust, actor authority, and disclosure policy: 018;
- common Finding code allocation, query APIs, evidence storage/indexing, invalidation, and scheduling: 019;
- exact reachability and artifact-placement semantics used by size groupings: 020;
- exact Runtime values, parts, functions, locale-service variation, diagnostics, and resource-limit semantics: 023;
- Target Profile capability vocabulary, output kinds, artifact relationships, and physical target-path identity: 024;
- Release evidence requirements, publication policy, activation, and rollback: 025;
- reference Runtime owner phase/cost registry, timer locations, allocator implementation, and cache workload profiles: 027;
- concrete component Workspace types, arena decisions, collection/data layouts, capacity heuristics, and fast-path implementations: their owning component designs;
- exact profiler public API, macro spelling, package layout, local report format, commands, report locations, CI workflow, dashboard, baseline storage, and retention: 029 and the implementing component designs;
- framework projection and Browser/SSR application-level hydration fixtures: 030;
- target-specific runner classes and numeric budgets: 032, 033, and 035; and
- any compatibility benchmark for legacy resource/catalog paths: 036.

No public command, package, artifact name, schema field spelling, budget value, runner vendor, or dashboard is reserved merely by an illustrative name in this design.

## Relationship to Other Documents

| Document | Relationship |
| --- | --- |
| [000 — Intlify overview](./000-intlify-overview-design.md) | Defines the product architecture, O6/O9/O11/O12/O15/O16 outcomes, I0 common measurement foundation, and cross-platform direction verified here |
| [001 — ox-mf2 toolchain foundation](./001-ox-mf2-toolchain-foundation.md) | Defines phase-separated, benchmark-driven design and product-owned result schemas preserved by Measurement Projections |
| [002 — parser and performance](./002-ox-mf2-phase-1-rust-parser-design.md) | Supplies parser measurement experience, workload separation, and external-comparison cautions without becoming the common schema |
| [013 — resource adapter](./013-ox-mf2-resource-catalog-adapter-design.md) | Supplies owner-defined extraction measurements and imported-boundary precedent |
| [014 — message linker](./014-ox-mf2-message-linker-design.md) | Supplies checked phase/cost, interval, workload, checksum, artifact-size, and result-integrity patterns retained under owner authority |
| [015 — Project Profile and locale policy](./015-intlify-project-profile-and-locale-policy-design.md) | Owns resolver phase/cost boundaries, workload vectors, and semantic checksums; 026 supplies common units, projection, comparison, and budget policy |
| [017 — shared artifacts](./017-intlify-shared-artifact-and-version-admission-design.md) | Owns physical representation and version admission for the semantic evidence model defined here |
| [018 — security and provenance](./018-intlify-security-trust-and-provenance-design.md) | Owns trust and authorization of evidence producers and consumers |
| [019 — project graph and Findings](./019-intlify-project-graph-query-and-incremental-design.md) | Owns Finding semantics, evidence dependencies, invalidation, and query projection checked here |
| [020 — requirement planning and linking](./020-intlify-requirement-planning-and-linking-design.md) | Supplies reachability, fallback, placement, and pruning facts used by conformance and artifact-size fixtures |
| [023 — localization execution](./023-intlify-localization-execution-specification-design.md) | Owns the logical execution results and resource semantics used by Runtime and cross-engine conformance |
| [024 — Target Profile and export](./024-intlify-target-profile-and-export-design.md) | Owns capabilities, output identities, and artifact relations measured and certified here |
| [025 — Release Assembly and deployment](./025-intlify-release-assembly-and-deployment-design.md) | Owns Release and hydration-group conditions that may consume 026 evidence |
| [027 — reference Runtime](./027-intlify-reference-runtime-design.md) | Produces one physical Runtime's conformance, initialization, loading, formatting, size, and memory evidence |
| [028 — JavaScript/Web vertical slice](./028-intlify-javascript-web-vertical-slice-design.md) | Supplies the first end-to-end Web subject and footprint baseline |
| [029 — product workflow](./029-intlify-product-workflow-and-packaging-design.md) | Owns commands, CI integration, evidence persistence, and presentation |
| [030 — Vue and SSR integration](./030-intlify-vue-ssr-tooling-integration-design.md) | Supplies framework-specific hydration subjects and projections |
| [032 — iOS target](./032-intlify-ios-target-design.md) | Supplies Apple target runner classes, capability requirements, and budgets |
| [033 — Android target](./033-intlify-android-target-design.md) | Supplies Android target runner classes, capability requirements, and budgets |
| [035 — Native and system targets](./035-intlify-native-system-target-design.md) | Supplies native runner classes, final-binary footprint, capability pruning, and target budgets |
