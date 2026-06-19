# ox-mf2 Toolchain Foundation 設計

## 目的

ox-mf2 は MessageFormat 2.0 (MF2) の高性能 parser であるだけでなく、将来的に lint、format、compile、diagnostics、bindings を支える MF2 toolchain foundation として設計する。

初期実装は parser に集中する。ただし、token、trivia、span、NodeId、diagnostics、table boundary は、後から tool を追加しても foundation を壊さないために、初期設計の一部として扱う。

## 設計概要

- Rust core を唯一の semantic implementation にする。
- Phase 1 では、recovering / lossless / snapshot-friendly な parser foundation を構築する。
- Phase 2 では、versioned Binary AST snapshot を public CST/AST view の標準境界にする。
- N-API と WASM を主要な language binding target にする。
- SemanticView は lossless Binary AST snapshot とは分離し、NodeId / Span にリンクする。
- MessagePack は AST 表現ではなく、将来の LSP/editor transport 用に予約する。
- Phase 1 の Rust parser / AST / performance 詳細は [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md) に置く。
- Phase 2 の Binary AST、snapshot、binding、transport の実装寄りの詳細は [003-ox-mf2-phase-2-binary-ast-bindings-design.md](./003-ox-mf2-phase-2-binary-ast-bindings-design.md) に置く。

## 設計思想

### MF2 toolchain foundation として設計する

![ox-mf2 toolchain foundation](./assets/001-ox-mf2-toolchain-foundation.svg)

ox-mf2 の中心的な設計原則は **MF2 toolchain foundation** である。

parser は中心的な存在だが、目的は単に最速の standalone parser を作ることではない。同じ core model の上に lint、format、compile、runtime validation、editor integration、benchmarking を載せられる foundation にする。

### oxc の高性能設計思想を継承する

ox-mf2 は oxc が提供する crate を一部利用する。ただし、これは crate の再利用だけではない。phase separation、data-oriented tables、allocation control、benchmark-driven design といった oxc の高性能設計思想も明示的に継承する。

- phase separation: lexer、parse、semantic lower、diagnostics、format、lint の各 phase を独立して計測できるようにする。
- data-oriented tables: pointer traversal だけに依存せず、NodeId と flat indexed tables によって後続処理を高速にする。
- stable identifiers: AST/CST node、token、source を ID で参照できるようにする。
- allocation control: parse phase 中の不要な heap allocation を避ける。
- benchmark-driven design: end-to-end performance だけでなく、内部 phase ごとの性能も計測する。

ただし、ox-mf2 は oxc と同じ arena typed AST model をそのまま採用しない。MF2 は JavaScript/TypeScript より構文面が小さく、国際化メッセージフォーマットとして formatting / linting の重要度が高いため、flat indexed tables を主要表現にする。

### core を toolchain へ拡張できるようにする

既存の dedicated parser toolchain から、parser、CST/AST、semantic analysis、diagnostics を core に置き、CLI、LSP、formatter、linter、外部 toolchain integration を adapter として周囲に置く構成が有効であることが分かる。

ox-mf2 も同じ方向を取る。MF2 専用 parser、CST、semantic model、diagnostics を core に置き、外部 toolchain integration は adapter として設計する。これにより core を MF2 に集中させつつ、Node bindings、CLI、LSP、各種 linter integration へ拡張できる。

### Binary AST は初期内部表現ではない

ox-jsdoc や typescript-go の Binary AST-style design は、bindings、snapshots、persistence、高速 transfer の参考になる。

ただし、ox-mf2 は Binary AST を最初の primary internal representation にはしない。Phase 1 の tool-facing syntax boundary は NodeId、TokenId、Span、accessors を中心にする。これにより table boundary を保ち、parser construction path を Binary AST-first に強制せず、Phase 2 で public AST view を Binary AST snapshot へ移行できる。

## 合意済み設計判断

### 初期責務

ox-mf2 は `toolchain foundation` である。

parser を最初に実装するが、token、span、accessor、table boundary の設計も初期段階から含める。これにより、後から linting、formatting、compilation を追加できる。

### 構文木

`Lossless CST + SemanticModel` を採用する。

```text
source
  -> lexer / token stream + trivia
  -> lossless CST
  -> SemanticModel / SemanticView
  -> linter / formatter / compiler
```

formatter は主に CST、tokens、trivia を使う。linter と compiler は主に semantic model を使う。

### Parser の error handling

`recovering parser` を採用する。

syntax error が見つかっても、parser は可能な範囲で CST を構築し、diagnostics を返す。fatal な欠落がある場合、SemanticModel は部分的に生成されるか、まったく生成されないことがある。

Phase 1 の result shape、recovery behavior、diagnostic cost model の詳細は [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md) で定義する。

### 内部メモリ表現

![ox-mf2 internal memory representation](./assets/001-ox-mf2-internal-memory-representation.svg)

`flat indexed tables` を採用する。

core identifiers は stable な `u32` index を使い、span は UTF-8 byte offset を使う。同じ identifier model を construction-time CST tables、将来の Binary AST snapshot、SemanticView、diagnostics、formatter、linter、language bindings で共有する。

linter、formatter、compiler は typed node struct に直接依存しない。NodeId と accessors を通して読む。

internal tables は snapshot-friendly にする。ox-mf2 では、public typed AST を先に構築してから再帰的に Binary AST へ変換する設計を避ける。代わりに、parser と lowering phase は table-oriented records を生成し、SnapshotWriter が nodes、edges、tokens、trivia、inline span fields、strings、diagnostics を linear pass で emit できるようにする。

Phase 1 の Rust tool は construction-time flat indexed tables を直接扱ってよい。Phase 2 以降は、Rust、N-API、WASM、その他 consumer で共有される Binary AST decoder/accessor view を canonical public AST view にする。これにより public AST surface を言語間で揃えつつ、parser は効率的な internal construction tables を使える。

Phase 1 の table contract、identifier model、source/span rules の詳細は [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md) で定義する。Phase 2 の Binary AST snapshot layout は [003-ox-mf2-phase-2-binary-ast-bindings-design.md](./003-ox-mf2-phase-2-binary-ast-bindings-design.md) で定義する。

### Formatter（整形）

`format-preserving first` を採用する。

formatter 自体は初期 MVP に含めなくてもよい。ただし、parser/table layer は token、trivia、original lexeme、delimiter span、recovery node、source-map-like information を保持し、後から formatter を構築できるようにする。

Phase 2 以降、formatter の public AST input は Binary AST decoder/accessor view にする。Rust 実装は必要に応じて construction-time tables の internal fast path を持ってよいが、stable な public formatter surface は Rust、N-API、WASM consumer で共有される Binary AST view に揃える。

将来の formatter は少なくとも次の 2 mode を支援する。

- preserve mode: 可能な限り元の表現を保持する。
- canonical mode: 標準的な ox-mf2 style に整形する。

### Linter（検査）

`diagnostics foundation` を採用する。

初期 MVP で多くの lint rule を実装する必要はない。ただし、parser error と lint diagnostics が同じ foundation を使えるように、diagnostic model を先に設計する。

Phase 2 以降、linter の public AST input は Binary AST decoder/accessor view にする。Rule implementation は Rust 内部の semantic fast path を使ってよいが、rule-facing / binding-facing traversal は、実用上可能な範囲で同じ public Binary AST view に寄せる。

core diagnostics は SourceId と UTF-8 byte Span を canonical location model にする。Label も byte span を持つ。CLI、LSP、editor integration は SourceStore を通して span を line/column または UTF-16 position に変換する責務を持つ。

concrete diagnostic shape と success-path cost constraints は [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md) で定義する。

### SemanticModel / SemanticView（意味情報モデル）

![ox-mf2 semantic model and semantic view](./assets/001-ox-mf2-semantic-model-view.svg)

`shared semantic model` を採用する。

これは runtime execution 直前の低レベル IR ではない。linter、compiler、validation が共有する semantic information model である。

Phase 2 以降、public semantic surface は SemanticView とし、semantic facts を Binary AST NodeId と Span にリンクする。semantic information を初期 Binary AST snapshot に無理に入れない。Binary AST は lossless CST surface を扱い、SemanticView は declarations、references、selectors、variants、fallback/default information、duplicate keys、coverage metadata などの semantic facts を扱う。

候補となる内容:

- symbol table
- variable declarations
- variable references
- function annotations
- selector list
- variant matrix
- fallback/default variant
- duplicate key set
- reachability / coverage metadata
- source span mapping

### Language binding（言語 binding）

![ox-mf2 language binding architecture](./assets/001-ox-mf2-language-binding.svg)

`Rust core as the single semantic implementation` を採用する。

ox-mf2 は MF2 parsing、CST construction、semantic analysis、diagnostics、formatting、linting を target language ごとに再実装しない。Rust core を MF2 semantics の唯一の実装にし、各 language binding はその core を包む薄い ergonomic wrapper にする。

初期 MVP では N-API、WASM、C ABI、その他 language bindings は必須ではない。ただし、Rust core の external API は最初から binding-friendly な形で設計する。

binding 実装の優先順位:

1. N-API binding: intlify と JavaScript tooling integration の主要 Node.js target
2. WASM binding: browser、playground、editor extension、edge runtime integration 向けの portable target
3. C ABI binding design: 将来の Go、Swift、C#、Zig、Python FFI、より広い native language integration の foundation

Rust internal types は他言語へ直接 expose しない。Binary AST decoder/accessor view、DiagnosticView、encoded snapshot view のような boundary type を許容する設計にする。

binding layer は ergonomic surface であり、MF2 semantics を重複実装する場所ではない。JS、WASM、C ABI、Go、Swift、C#、その他 consumer は同じ Rust core を呼び出し、stable view、handle、diagnostics、formatted text、encoded snapshot を受け取る。

language boundary を越えて full CST/AST output を返す場合、Phase 2 以降の canonical product boundary は nested JSON AST ではなく versioned Binary AST snapshot にする。Debug JSON や compatibility JSON は存在してよいが、standard hot-path representation にはしない。

Phase 2 の Binary AST snapshot は lossless CST surface に集中する。semantic information は SemanticView または後続の semantic snapshot として別に expose する。N-API と WASM bindings は eager に materialized object tree を返すのではなく、lazy decoder/accessor を持つ result object を返す。raw snapshot bytes は default result に含めず、advanced/debug/transport API で明示的に取得できるようにする。

MessagePack は ox-mf2 の CST/AST representation ではない。LSP、editor integration、daemon mode、repeated semantic queries のような long-lived language-service workflow における future transport として予約する。

Binary AST、binding、snapshot、transport の詳細設計は [003-ox-mf2-phase-2-binary-ast-bindings-design.md](./003-ox-mf2-phase-2-binary-ast-bindings-design.md) で定義する。

### Parser API

`parse_source + SourceStore` を採用する。

SourceStore は single parse、batch parse、diagnostics、editor boundary、将来の snapshot roots section に共通する source ownership layer である。convenience API も内部で source text を登録し、SourceId 経由で処理する。

MF2 workloads では、1 file、1 locale set、1 project に多数の message が含まれることが多いため、batch parsing は first-class API にする。path、locale、message_id、base_offset などの batch metadata は identity、diagnostics、fixtures、benchmarks、将来の snapshot root mapping のために使う。parser semantics を変えてはならない。

Phase 1 の parser APIs、SourceStore contract、ParseInput metadata、ParseOptions defaults、result types の詳細は [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md) で定義する。snapshot-producing APIs は [003-ox-mf2-phase-2-binary-ast-bindings-design.md](./003-ox-mf2-phase-2-binary-ast-bindings-design.md) で定義する。

### Suppression / directive comment（診断抑制）

`diagnostic suppression boundary only` を採用する。

初期段階では、ox-mf2 は MF2 内の具体的な directive comment syntax を固定しない。ただし、diagnostic pipeline には diagnostics を suppress できる boundary を持たせる。

suppression は parser syntax policy ではなく diagnostic-layer concern として扱う。concrete suppression data shape は linter と language-service workflow が implementation phase に入った時点で定義できる。

### Benchmark（性能計測）

`phase-separated benchmark` を採用する。

CLI 全体に対する hyperfine measurement に加えて、internal performance を phase ごとに見えるようにする。

対象 phase:

- lexer
- parse_cst
- lower_semantic
- diagnostics
- encode_snapshot
- decode_snapshot
- snapshot_accessor_traversal
- snapshot_to_bytes_copy
- binding_call
- parse_batch_to_snapshot
- format_preserve
- format_canonical
- e2e_parse
- e2e_lint

Phase 1 parser / AST / performance design は [002-ox-mf2-phase-1-rust-parser-design.md](./002-ox-mf2-phase-1-rust-parser-design.md) で詳述する。

### Crate 構成

`core split minimal` を採用する。

初期候補:

```text
crates/
  ox_mf2_syntax        # lexer, token, CST, parser, recovery
  ox_mf2_semantic      # semantic model, symbol/reference/selector/variant analysis
  ox_mf2_diagnostics   # Diagnostic, Severity, Label, suppression boundary
  ox_mf2               # facade API
```

将来候補:

```text
ox_mf2_linter
ox_mf2_formatter
ox_mf2_codegen
ox_mf2_cli
ox_mf2_napi
ox_mf2_wasm
```

### 仕様追跡

`Unicode spec primary + TC39 proposal tracking` を採用する。

primary source:

- `refers/message-format-wg/spec`

tracked source:

- `refers/proposal-intl-messageformat`

MF2 syntax と message data model は主に Unicode WG spec に従う。Intl.MessageFormat API integration と ECMAScript 側の挙動は TC39 proposal を追跡する。

### Conformance test（仕様適合性テスト）

`spec fixtures + implementation fixtures` を採用する。

```text
fixtures/
  spec/
    unicode-wg/
    tc39/
  implementations/
    formatjs/
    messageformat/
    mf2-tools/
    ox-content/
  recovery/
  formatter/
  diagnostics/
```

spec fixtures は conformance checks の基礎であり、implementation fixtures は compatibility と diff detection のために使う。

spec fixtures は Unicode Message Format WG spec と TC39 proposal に基づく。目的は、ox-mf2 が spec 上 valid な MF2 を受け入れ、spec 上 invalid な syntax を拒否することを確認することである。つまり、spec fixtures の結果は parser conformance を表す。

implementation fixtures は既存 parser implementation と real-world messages に基づく。目的は、既存実装が accept/reject する case と ox-mf2 の差分を観測することであり、edge cases、error recovery、MF1 compatibility cases、real project messages を含む。implementation fixtures の結果は spec conformance を定義しない。compatibility information、diff detection、design-decision input として扱う。

たとえば、spec fixtures は次のように構成できる。

```text
fixtures/spec/unicode-wg/valid/local-declaration.mf2
fixtures/spec/unicode-wg/valid/matcher-select.mf2
fixtures/spec/unicode-wg/invalid/unclosed-expression.mf2
fixtures/spec/tc39/valid/intl-messageformat-api-options.mf2
```

implementation fixtures は次のように構成できる。

```text
fixtures/implementations/messageformat/accepted-edge-cases.mf2
fixtures/implementations/mf2-tools/error-recovery-cases.mf2
fixtures/implementations/formatjs/mf1-compat-cases.mf1
fixtures/implementations/ox-content/real-world-messages.mf2
```

implementation fixtures は specification の代替ではない。他の実装が message を受け入れても、それが Unicode WG spec または TC39 proposal に違反する場合、ox-mf2 は拒否してよい。ただし、その差分は compatibility input として記録する。

## 初期アーキテクチャ

![ox-mf2 initial architecture](./assets/001-ox-mf2-initial-architecture.svg)

## Table Boundary（テーブル境界）

table boundary は、internal table representation と tool-facing API の境界である。

![ox-mf2 table boundary](./assets/001-ox-mf2-table-boundary.svg)

この boundary により、初期実装では flat indexed tables を使いながら、後から public AST view が Binary AST-style compact tables に移行しても、linter、formatter、compiler API をおおむね安定させられる。

## Phase Plan（フェーズ計画）

### MVP / Phase 1

最初の phase は parser foundation に集中する。

- lexer
- recovering CST parser
- diagnostics model
- conformance fixtures
- compatibility observation のための implementation fixtures
- phase-separated benchmark
- parser performance design
- snapshot-friendly flat indexed tables
- SourceStore / SourceId を持つ Rust facade API
- Rust batch parsing API shape

### Phase 2

2 番目の phase では cross-language product boundary を追加する。

- versioned Binary AST snapshot
- SnapshotWriter
- roots、sources、nodes、edges、tokens、optional trivia、diagnostics、diagnostic labels、string table、optional source text data、optional extended data の lossless CST snapshot sections
- spans は separate section ではなく NodeRecord、TokenRecord、TriviaRecord、DiagnosticRecord に inline で保持する
- Rust Binary AST decoder / accessor API
- lazy decoder / accessor API を持つ N-API binding
- portable decoder / accessor API を持つ WASM binding
- shared snapshot buffer と shared string table を持つ first-class parseBatch API
- SemanticView または将来の semantic snapshot として分離 expose される semantic model
- stable C ABI implementation を必須にしない C ABI design preparation
- snapshot encoding / decoding / binding benchmarks

### Phase 3

3 番目の phase では ox-mf2 をより広い tooling workflow へ拡張する。

- formatter expansion
- linter expansion
- language service / LSP model
- editor workflow cache と repeated query model
- internal language-service sessions 用の optional MessagePack transport
- parser、semantic、snapshot、transport、binding costs を分離する editor workflow benchmarks

## 非目標

初期段階では次を実装対象にしない。

- Phase 1 における Binary AST-first internal representation
- full linter ruleset
- canonical formatter
- N-API / WASM binding
- MessagePack transport
- complete Intl.MessageFormat runtime

ただし、初期設計には後から追加するために必要な boundary を含める。
