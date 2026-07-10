# `design/` 設計ドキュメント矛盾チェック

- 実施日: 2026-07-10
- 対象: `design/*.md` 15件、`design/linter-rules/*.md` 11件、`design/assets/*.svg` 29件
- 観点: 文書内の自己矛盾、文書間の公開契約・所有権・データ形式・失敗境界の矛盾、本文と参照図の矛盾
- 判定: 確定した矛盾 20件、解釈を明文化すべき契約衝突 6件

明示的に「将来」「deferred」「Phase N では非対応」と区別されている差は、現在の契約と衝突しない限り指摘から除外した。後続文書が正本であると明記されていても、先行文書や参照図に相反する規範が残っている場合は、実装者が両方を参照するため矛盾として扱った。

## サマリー

| ID | 重要度 | 概要 |
| --- | --- | --- |
| C-01 | Critical | Binary AST の参照図が現行 v0.1 のヘッダー長・TokenRecord 長と不一致 |
| C-02 | High | パーサーAPIが、同じ文書で要求する fatal/resource API error を返せない |
| C-03 | High | SemanticModel の診断所有権と構築条件が本文と図で逆転している |
| C-04 | High | 空の Trivia セクションが、収集していない trivia の能力証明にもなってしまう |
| C-05 | High | SnapshotWriter の SourceId map 記述が「1 root = 1 SourceRecord」と両立しない |
| C-06 | High | parse artifact cache のキーが ParseOptions と parser version を反映しない |
| C-07 | High | cache が SourceStore を破棄した後も store-local SourceId を保持する |
| C-08 | High | formatter IR が semantic error である variant arity を grammar invariant と扱う |
| C-09 | Medium | Document IR 図だけが message-level renderer に final LF を追加する |
| C-10 | High | operational error の格納先が top-level only と `results[].errors` に分裂している |
| C-11 | High | formatter binding の parser diagnostic shape が numeric と string の二契約を持つ |
| C-12 | Medium | Snapshot kind accessor が public numeric と public symbolic の二契約を持つ |
| C-13 | Medium | formatter の snapshot version error が実際の major/minor 形式を表せない |
| C-14 | Medium | unpaired surrogate が `TypeError` と `SourceTextErrorCode` の両方に割り当てられている |
| C-15 | Medium | LSP の stale edit 動作が確定事項と open question の両方に存在する |
| C-16 | High | `formatSnapshot` の `source` が省略可能と必須の二契約を持つ |
| C-17 | High | 初期 formatter が `.editorconfig` を読むかどうかが文書間で逆転している |
| C-18 | High | SemanticModel construction が semantic error を検出するか facts のみを収集するかが不一致 |
| C-19 | Medium | foundation の formatter pipeline が SemanticModel 経由と CST/SnapshotView 直結の二通りある |
| C-20 | Medium | `recommended` を correctness 中心とする説明と、実際の preset metadata が一致しない |

## 確定した矛盾

### C-01: Binary AST の参照図が現行 v0.1 wire format と不一致

根拠:

- `design/003-ox-mf2-phase-2-binary-ast-snapshot-design.md:301-319` は `SnapshotHeader = 32 bytes`、`TokenRecord = 36 bytes` と定義し、`reserved_tail` を含める。
- `design/003-ox-mf2-binary-ast-format-changelog.md:14-15` も 32/36 bytes を互換性契約として固定する。
- `design/assets/003-ox-mf2-binary-ast-format-layout.svg:38-43,68-70` は 28 byte header + 4 byte padding、TokenRecord 32 bytes と描く。
- `design/assets/003-ox-mf2-wire-layout.svg:20-21,62-67` も 28 byte header + padding とし、`reserved_tail` を持たない。

矛盾: 同じ設計から参照される図が、本文の固定オフセットと record size を4 bytesずつ異なる値で示している。

影響: 図を基に writer/decoder、他言語 decoder、golden fixture を実装すると、section offset と token traversal が壊れる。

解消案: 本文と changelog を正本として両SVGを再生成し、header byte 28..32 を `reserved_tail: u32`、TokenRecord を 36 bytes に更新する。

### C-02: パーサーAPIが規定された API error を表現できない

根拠:

- `design/002-ox-mf2-phase-1-rust-parser-design.md:514-524` の `parse_source`、`parse_message`、`parse_source_session` は `ParseResult` / `ParseSessionResult` を直接返す。
- 同文書 `:682-695` の result shape に API error field はない。
- 同文書 `:851-853` は root すら構築できない fatal failure を API error とする。
- 同文書 `:1053-1057` は trivia count overflow を parser diagnostic ではない resource-limit API error とする。

矛盾: `Result<..., ParseError>` でも error field でもないため、明記された failure を panic、diagnostic、sentinel のどれにもせず返す方法がない。

影響: Rust API と bindings で panic、partial result、未記載 error type に分岐し、batch の全体失敗/個別失敗も定まらない。

解消案: parse API を typed `Result` にするか、fatal/resource failure を含む明示的 result enum を定義し、batch failure と error code domain も固定する。

### C-03: SemanticModel の診断所有権と構築条件が本文と図で逆転している

根拠:

- `design/002-ox-mf2-phase-1-rust-parser-design.md:287` は SemanticModel が semantic facts のみを所有するとする。
- `design/012-ox-mf2-parser-semantic-validation-design.md:34-54` は `validate_semantics(model)` が診断を別返しし、parser diagnostics がある result からの model 構築を misuse とする。
- `design/assets/002-ox-mf2-semantic-model-design.svg:117-119,157-159` は builder が duplicate/missing check を行い、SemanticModel 自体に diagnostics を格納する。
- `design/assets/001-ox-mf2-initial-architecture.svg:70-108` は semantic lowering/model 構築後に diagnostics を生成する一本道を描く。

矛盾: canonical prose は facts と diagnostics を分離し parser diagnostics 時の構築を禁止するが、参照図は model が diagnostics を所有し、model 構築後に診断を作る。

影響: semantic diagnostics の二重保存、`ParseResult.diagnostics` への混入、invalid CST からの SemanticModel 構築が起き得る。

解消案: 図から SemanticModel 内の diagnostics を除去し、`parser diagnostics == empty` を builder の前提として描く。

### C-04: Trivia section presence が実際の trivia 能力を証明しない

根拠:

- `design/003-ox-mf2-phase-2-binary-ast-snapshot-design.md:156-158` は `collect_trivia = false` でも `include_trivia = true` なら空の Trivia section を capability marker として出す。
- 同文書 `:281-287` は section presence を trivia が encode された能力証明とする。
- `design/007-ox-mf2-phase-3b-formatter-design.md:910-918` は preserve mode に token-level trivia を要求する。
- `design/004-ox-mf2-phase-2-language-bindings-design.md:227-231` は収集していない trivia は encode できないため、この option 組合せを `TypeError` とする。

矛盾: Rust snapshot contract では、未収集 trivia と、収集した結果0件だった trivia が同じ present/count=0 の能力証明になる。binding だけは不正な組合せを拒否する。

影響: Rust 経由の lossy snapshot を preserve formatter が完全な snapshot と誤認し得る。

解消案: Rust API でも組合せを拒否する、未収集なら section absent にする、または独立した collected capability flag を追加する。

### C-05: SnapshotWriter の SourceId map が v0.1 no-dedup policy と両立しない

根拠:

- `design/003-ox-mf2-phase-2-binary-ast-snapshot-design.md:217` は一時的な `Phase 1 SourceId -> snapshot-local SourceId` map を保持するとする。
- 同文書 `:636-638` は v0.1 writer が SourceRecord を deduplicate せず各 root に1件割り当てるとする。
- `design/003-ox-mf2-binary-ast-format-changelog.md:57` は2つの root が同じ Phase 1 SourceId を持っても別 SourceRecord を emit するとする。

矛盾: 同じ Phase 1 SourceId に複数の snapshot-local SourceId を割り当てるため、単純な一対一 map では表現できない。

影響: shared SourceId を持つ batch で source identity が誤って deduplicate され、token/trivia/diagnostic の参照先もずれ得る。

解消案: root occurrence ごとの source slot vector とするか、key を `(root occurrence, Phase 1 SourceId)` にする。

### C-06: Parse artifact cache の key が parse 結果を一意にしない

根拠:

- `design/ox-mf2-parse-artifact-cache.md:21-31` の key は `(source_id_namespace, message_id, source_hash)` のみである。
- `design/002-ox-mf2-phase-1-rust-parser-design.md:666-678` の `recovery`、`parse_semantic`、`collect_trivia` は結果を変える。
- cache skeleton は options を受け取るが `design/ox-mf2-parse-artifact-cache.md:68-84` で options を見る前に hit を返す。
- 同文書 `:53-65` は parser version ごとの一意性を要求する一方、`parser_version` を lookup/invalidation に使わない。
- 同文書は source hash が衝突してはならないとしながら `:127-129` で xxhash を許し、hit 時の source equality check を定義しない。

矛盾: 同じ key で semantic/trivia/recovery/parser version が異なる artifact を要求でき、hash-only key で never collide も保証できない。

影響: 情報不足の CST、旧parser result、別sourceのartifactを返す可能性がある。

解消案: parser version と結果に影響する ParseOptions を key に含め、hit 時に source bytes も比較する。

### C-07: Cache が破棄済み SourceStore の SourceId を保持する

根拠:

- `design/ox-mf2-parse-artifact-cache.md:35-49` は SourceId を stable within the SourceStore としながら、fresh SourceStore に cached span を戻せるとする。
- skeleton `:77-94` は temporary SourceStore の id を CachedParse に保存した直後に SourceStore を drop する。
- `design/002-ox-mf2-phase-1-rust-parser-design.md:785-810` は SourceStore が source text、line index、source identity の owner だとする。

矛盾: store-local identity の owner を破棄しているため、その SourceId を別の fresh store で同一 identity として扱う根拠がない。

影響: diagnostic/token/trivia の SourceId が orphan になり、line/column 解決が偶然同じ数値 ID の再割当てに依存する。

解消案: cache が SourceStore/source owner を保持するか、fresh store の新 SourceId へ全 record/diagnostic を remap する。

### C-08: Formatter IR が semantic arity error を grammar invariant と扱う

根拠:

- `design/011-ox-mf2-formatter-ir-design.md:253,533` は各 matcher row の key 数が selector 数と等しいことを grammar invariant とし、不一致を internal error にする。
- `design/012-ox-mf2-parser-semantic-validation-design.md:334-350` は parser が任意の key count を構文上受理し、不一致を `variant-key-arity-mismatch` semantic diagnostic とする。
- formatter は parser diagnostics を gate にするため、semantic validation 前の grammar-valid/semantic-invalid CST が formatter IR に到達し得る。

矛盾: 同じ input shape が semantic user error と formatter implementation invariant failure の両方に分類されている。

影響: 正当な semantic error input が `internal_error` になり、formatter failure boundary を破る。

解消案: formatter 前に semantic diagnostics も gate するか、IR が arbitrary arity を安全に扱い、semantic-invalid input の明示的 failure を返す。

### C-09: Document IR 図だけが message-level output に final LF を追加する

根拠:

- `design/assets/011-ox-mf2-document-ir.svg:106-114` は renderer result を exactly one final LF と描く。
- `design/007-ox-mf2-phase-3b-formatter-design.md:57,209,405,868` は message-level API が final newline を追加せず、CLI file framing だけが LF を付けるとする。

矛盾: 図では Document renderer 自体が LF を追加するが、本文では renderer/message-level output は unframed である。

影響: binding API の output/`changed` 判定が CLI framing を誤って含み、末尾改行のない message を常に変更扱いにし得る。

解消案: SVG の renderer result から final LF を削除し、CLI file-framing box を別に描く。

### C-10: Operational error の JSON 格納先が二重契約になっている

根拠:

- `design/006-ox-mf2-phase-3a-tooling-foundation-design.md:268-288,322` は operational errors を top-level `errors` のみに格納するとする。
- `design/007-ox-mf2-phase-3b-formatter-design.md:550-569` は global error を top-level、file-specific error を `results[].errors` に格納する。
- `design/008-ox-mf2-phase-3c-linter-design.md:215,550-575` も file-specific operational error を `results[].errors` に格納しつつ Phase 3A envelope に従うとする。

矛盾: Phase 3A の only top-level 規則と、Phase 3B/3C の target-local error 規則が同じ shared envelope の契約として併存する。

影響: schema、consumer、agent integration が file error をどこから読むべきか決まらず、error count も変わる。

解消案: Phase 3A を global errors only に改訂し、`results[].errors` を command result extension として正式に定義する。

### C-11: Formatter binding の parser diagnostic shape が numeric と string の二契約を持つ

根拠:

- `design/004-ox-mf2-phase-2-language-bindings-design.md:463-475` の `DiagnosticView` は numeric `DiagnosticSeverity` / `DiagnosticCode` を持ち、category を持たない。
- `design/007-ox-mf2-phase-3b-formatter-design.md:197-200` は formatter N-API/WASM が parser package と同じ diagnostic JavaScript shape を再利用するとする。
- `design/008-ox-mf2-phase-3c-linter-design.md:152-178` は fmt/lint reporters と binding result objects が category、kebab-case string code、string severity のJSON shapeを共有するとする。

矛盾: formatter binding diagnostic が numeric parser shape と string reporter/lint binding shape の両方に指定されている。

影響: public TypeScript types、N-API/WASM parity、JSON serialization の互換性が定まらない。

解消案: programmatic parser/formatter shape と CLI/lint reporter shape を明示的に分離するか、全bindingを一つのshapeへ移行する。

### C-12: Snapshot kind accessor が public numeric と public symbolic の二契約を持つ

根拠:

- `design/004-ox-mf2-phase-2-language-bindings-design.md:558-576` は snapshot/diagnostic kind を numeric const object と numeric union で公開し、name helper を別に提供する。
- `design/007-ox-mf2-phase-3b-formatter-design.md:897-906` は public node/token kind accessor が stable symbolic name を公開し、numeric discriminant は internal とする。

矛盾: 同じ SnapshotView public accessor が numeric-first と symbolic-only の二通りに規定されている。

影響: formatterが既存Phase 2 accessorを直接使えるか、adapter変換が必要か、public compatibility surface が変わる。

解消案: Phase 2 numeric value を canonical public identity とし name helper をformatterで使うか、symbolic facade を新しい明示的adapterとして定義する。

### C-13: `invalid_snapshot` の version details が wire version を表現できない

根拠:

- Binary AST header は major/minor pair で v0.1 を表す。
- `design/007-ox-mf2-phase-3b-formatter-design.md:325-335` は `version: 3` と `supportedVersions: [1, 2]` という scalar integer だけの error details を定義する。

矛盾: error details の scalar version では `{ major, minor }` の wire version、特に v0.1 を曖昧なく表現できない。

影響: unsupported-version error の比較、fixture、将来の minor compatibility 判定が不明確になる。

解消案: `{ major, minor }` と `supportedVersions: [{ major, minor }]`、または canonical string `"0.1"` に統一する。

### C-14: Unpaired surrogate の error class/code が二重に割り当てられている

根拠:

- `design/004-ox-mf2-phase-2-language-bindings-design.md:235-250,401-418` は parser input と `withSources()` の unpaired surrogate を built-in `TypeError` とする。
- 同文書 `:495-537` は built-in type/range error と numeric-code付き dedicated error class を分離する。
- `design/appendix-ox-mf2-error-code.md:67-71` は unpaired surrogate rejection を `SourceTextErrorCode` の対象例に含める。

矛盾: 同じ失敗が code を持たない `TypeError` と、numeric SourceTextError の両方に属する。

影響: consumer が `instanceof TypeError` と `error.code` のどちらで処理するか、parity test が何を固定するか決まらない。

解消案: binding design を優先するなら appendix から削除し、numeric code を使うなら 004 の `TypeError` 規則を変更する。

### C-15: LSP stale edit の動作が確定事項と open question の両方にある

根拠:

- `design/005-ox-mf2-phase-3-tooling-transport-design.md:478-493` は stale document version/message mapping の場合に no-op とする。
- `design/009-ox-mf2-phase-3d-lsp-editor-design.md:54-62` も stale mapping は no-op と規定する。
- 同文書 `:94-96` は stale request を no-op にするか operational editor error にするかを open question とする。
- edit granularity も 007 が smallest practical edit を要求する一方、005/009 は whole message replacement を許し、009 で未決定としている。

矛盾: 同じ実装分岐が normative requirement と unresolved choice の両方として残っている。

影響: adapter ごとに stale request の通知有無や edit range が変わる。

解消案: Phase 3D 文書で決定を一つにし、決定済みなら open question を削除する。未決定なら既存の `should` を provisional wording にする。

### C-16: `formatSnapshot` の `source` が省略可能と必須の二契約を持つ

根拠:

- `design/005-ox-mf2-phase-3-tooling-transport-design.md:130-136` は `formatSnapshot(snapshot, source?, options?)` とし、型上 source を省略可能にする。
- 同じ段落は preserve mode、source slicing、parser diagnostics、editor position conversion に source text が必要とも述べる。
- `design/007-ox-mf2-phase-3b-formatter-design.md:174-184` は `formatSnapshot(snapshot, source, ...)` と `checkSnapshot` の両方で source を必須にする。

矛盾: overview API は source なしの呼び出しを許す一方、詳細 API と必要機能は source なしの成功経路を定義していない。

影響: TypeScript declaration、arity validation、Rust API、source 不在時の error behavior が実装ごとに分かれる。

解消案: 005 の `source?` を必須に変更する。省略経路を残す場合は利用可能な mode と capability error を定義する。

### C-17: 初期 formatter が `.editorconfig` を読むかどうかが文書間で逆転している

根拠:

- `design/005-ox-mf2-phase-3-tooling-transport-design.md:181-183` は formatter が未設定 option の fallback として `.editorconfig` を読むべきだとする。
- `design/007-ox-mf2-phase-3b-formatter-design.md:26-36` は、対応 option が存在する前の loading を initial non-goal にする。
- 同文書 `:675-687` は初期 option が mode のみなので読まず、line width や indent width 等の導入後に有効化するとする。

矛盾: 初期実装の config discovery/I/O 要件が「読む」と「読まない」で逆である。

影響: CLI実装、config dependency、fixtureの期待値が一致しない。

解消案: 005 を「初期は読まず、消費できる formatter option の導入時に fallback を有効化する」に変更する。

### C-18: SemanticModel construction が semantic error を検出するか facts のみを収集するかが不一致

根拠:

- `design/002-ox-mf2-phase-1-rust-parser-design.md:62-65` は duplicate declaration の syntax-adjacent case を parser non-responsibility から除外する。
- 同文書 `:287-308` は SemanticModel が diagnostics ではなく facts を所有するとした直後、construction task に duplicate/missing semantic anchor の detect を含める。
- 同文書 `:322-326` と `design/012-ox-mf2-parser-semantic-validation-design.md:34-44` は Data Model Error の診断生成を `validate_semantics(model)` に一元化する。

矛盾: duplicate/missing condition を construction が検出するのか、validation が facts から診断するのか定まらない。

影響: 二重検出、construction failure と SemanticDiagnostic の混同、診断順序/cascade policy の迂回が起き得る。

解消案: construction task を fact collection のみに直し、user-facing condition は 012 の validation に委譲する。

### C-19: Formatter pipeline が SemanticModel 経由と CST/SnapshotView 直結の二通りある

根拠:

- `design/001-ox-mf2-toolchain-foundation.md:69-75` の pipeline は lossless CST から SemanticModel/SemanticView を経て formatter に進む。
- 直後の `:77` は formatter が主に CST、tokens、trivia を使うとする。
- `design/011-ox-mf2-formatter-ir-design.md:39-57` は source/parser または SnapshotView から formatter traversal/IR に直接進む。
- `design/007-ox-mf2-phase-3b-formatter-design.md:42-77` も public syntax view と source shape/trivia に基づく。

矛盾: foundation の図式だけが formatter を SemanticModel の downstream に置き、本文と詳細設計は CST/SnapshotView の downstream に置く。

影響: formatter crate が SemanticModel construction/validation を必須 dependency と誤認し、制御とbenchmark stageが変わる。

解消案: pipeline を分岐させ、formatter は CST/SnapshotView、linter/compiler/validator は SemanticModel を主入力とする。

### C-20: `recommended` の説明と preset metadata が一致しない

根拠:

- `design/005-ox-mf2-phase-3-tooling-transport-design.md:251-255` は initial recommended を broadly useful correctness diagnostics 中心とする。
- `design/008-ox-mf2-phase-3c-linter-design.md:338-345,795-799` では recommended の2 rule は両方 `best-practice` / `warn` である。
- 唯一 `correctness` category の `no-undeclared-variable` は default off で recommended ではない。
- `design/linter-rules/index.md:43-47` も同じ metadata を再掲する。

矛盾: preset の category 方針は correctness 中心だが、実際の全 member は best-practice で correctness rule は member ではない。

影響: 将来の追加基準、documentation、schema description、release-note の互換性判断が異なる方針で運用される。

解消案: metadata を維持するなら 005 を broadly useful diagnostics に変更する。correctness 中心を維持するなら membership/category を再設計する。

## 解釈を明文化すべき契約衝突

以下は実装を二方向に分けるが、draft/future の例外と解釈すれば両立する余地がある。誤読を防ぐため明文化が必要である。

### R-01: v0.x の SyntaxKind 追加に major bump が必要か

- `design/002-ox-mf2-phase-1-rust-parser-design.md:127-128` は新しい core SyntaxKind の emit に major version change が必要とする。
- `design/003-ox-mf2-phase-2-binary-ast-snapshot-design.md:693` は v0.x 中は fixture/changelog 更新を要求し、major bump を必須にするのは v1.0 freeze 後と読める。
- `design/003-ox-mf2-binary-ast-format-changelog.md:83-88` は v0.x change を draft bump とし minor/major の選択を許す。

`0.1 -> 0.2` でよいのか `0 -> 1` が必要なのかを一文で固定する必要がある。

### R-02: ignored stdin は raw passthrough か framed output か

- `design/007-ox-mf2-phase-3b-formatter-design.md:392-405` は stdin も BOM/末尾改行を read-frame し、stdout は formatted message + exactly one LF とする。
- 同文書 `:480` は ignored `--stdin-filepath` では original stdin source を stdout に書くとする。

original が raw bytes なら framing の例外になり、unframed message text なら exactly one LF を追加する必要がある。byte-level fixture を明示すべきである。

### R-03: Agent が取得する docs slug の正本がない

- `design/010-ox-mf2-phase-3e-agent-integration-design.md:34-38` は agent が rule description、docs slug、remediation context を得る入口として `linter-rules/index.md` を指定する。
- `design/008-ox-mf2-phase-3c-linter-design.md:232-247` は docs slug を internal generated metadata とし、`design/linter-rules/*.md` は正本ではなく runtime metadata API も非公開とする。

Agent が必要とするのが design page path か将来の generated slug かを分け、初期契約では slug を要求しないか derivation を定義する必要がある。

### R-04: `format-preserving first` と default `standard` の関係

- `design/001-ox-mf2-toolchain-foundation.md:103-114` は `format-preserving first` を採用し、preserve を original representation を可能な限り保持するものとする。
- `design/007-ox-mf2-phase-3b-formatter-design.md:59-77` は preserve を minimal-diff ではないとし、local spacing、indent、matcher table 等を正規化する。
- 同文書 `:675-681` は public default を standard にする。

first が実装優先度、lossless foundation、user-facing default のどれかを明記し、製品 default と正規化範囲は 007 に委譲すべきである。

### R-05: Linter の common syntax input が SnapshotView か construction-time CstView か

- `design/005-ox-mf2-phase-3-tooling-transport-design.md:63-67` は SnapshotView が formatter、linter、LSP/editor、transport の common syntax input であり続けるとする。
- 同文書 `:267-273` は initial linter が source-backed で、RuleContext が CST access と SemanticModel facts を受け、`lintSnapshot` は future とする。
- `design/008-ox-mf2-phase-3c-linter-design.md:757-759` は snapshot-to-SemanticModel path がないため snapshot-backed linting を defer する。

common syntax input を将来の public foundation と読むなら両立するが、Phase 3C の実入力と読むと衝突する。initial linter の CstView 例外を明記すべきである。

### R-06: `errorCount` の意味が surface ごとに異なる

- `design/005-ox-mf2-phase-3-tooling-transport-design.md:279-289` は共有 result contract の summary 例に `errorCount` / `warningCount` を置く。
- `design/008-ox-mf2-phase-3c-linter-design.md:633-637` の CLI summary では `errorCount` は operational error 数で、診断数は `diagnosticErrorCount` / `diagnosticWarningCount` である。
- 同文書 `:684-705` の programmatic result では `errorCount` / `warningCount` が diagnostic severity 件数で、operational error は `ok: false` に分離される。

008 自体は差を意図的に説明しているが、005 の共有 contract が同名 field の共通意味を示すように読める。CLI envelope と message-level result を別表にすべきである。

## 再確認の結果、矛盾として数えなかった候補

| 候補 | 再判定 |
| --- | --- |
| 005 の CLI parallelism と 007/008 の initial sequential | `may` は許可であり必須ではなく、007/008 は将来の parallel 化も許すため両立する。 |
| 005 の preserve 対象に comment/literal spelling があること | `may preserve` は排他的な mode 差を要求しない。mode の意味の差は R-04 に統合した。 |
| 005 の optional UTF-16 position と 008 の binding shape | 005 は exact schema を 008 に委譲し、position を optional かつ CLI/editor consumer 向けとするため両立する。 |
| public AST input が Binary AST で主 API が source text | AST/view の形式と entry point の引数は別の境界であり、005 自身が source-first と snapshot reuse path を併記している。 |
| diagnostic schema に関する重複候補 | formatter binding まで単一shapeとする場合の矛盾は C-11 に統合した。lint product 内のshapeは008で揃っている。 |

## 確認済みで矛盾なしと判断した主な領域

- core semantic diagnostic 7件の code、severity、configurability、catalog ordering
- configurable rule 3件の default severity と recommended membership（005 の方針説明との差は C-20）
- parser → semantic → rules の short-circuit pipeline（C-03/C-18 の古い図・残存文言を除く本文同士）
- UTF-8 byte Span、JSON location、LSP UTF-16 conversion の責務分離
- Phase 2 snapshot の section kind、現行本文上の record size、optional section policy（C-01/C-04/C-05 を除く）
- formatter/linter の file discovery、ignore precedence、exit-code priority
- Phase 3C source-backed lint と future snapshot-backed lint の区別

## 推奨修正順

1. C-01、C-04、C-05: wire/capability/source identity は互換性とデータ破損に直結する。
2. C-02、C-06、C-07: API/cache の型と lifetime を実装前に固定する。
3. C-08、C-03、C-18: parser/semantic/formatter の correctness boundary を揃える。
4. C-10〜C-14、C-16、C-17: CLI/binding の公開 shape と初期要件を一つにする。
5. C-19、C-20: foundation/overview と詳細製品契約を揃える。
6. C-09、C-15、R-01〜R-06: 図・editor/agent・draft policy の残存差を解消する。

## レビューと検証

- `design/` 配下の Markdown 26件と参照SVG 29件を横断確認した。
- レポート内で参照した design file の存在を確認した。
- `vp install` を実行した。
- `vp check` は成功した。
- `vp test` は 25 test files 成功、1 skipped、110 tests 成功、8 skipped だった。
