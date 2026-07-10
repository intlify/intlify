# `design/` レビュー統合・妥当性確認

- 実施日: 2026-07-10
- 入力レポート:
  - `.reports/design-contradiction-review-codex.md`
  - `.reports/design-contradiction-review-grok.md`
  - `.reports/design-contradiction-review-claud.md`
- 原文確認対象: `design/` 配下の Markdown 26件、`design/assets/` の SVG 29件
- 注記: 依頼文の `designs/` はリポジトリに存在しないため、実在する `design/` を対象とした。

## 判定方法

3レポートの全見出しを抽出し、同じ問題を別名で報告しているものを統合した。各主張は、引用された行だけでなく、その節の scope、phase、`deferred` / `future`、正本の委譲先、参照SVGまで戻って確認した。

判定は次の4区分とする。

- **確定**: 同時に満たせない normative contract、内部論理の欠落、または事実と異なる図/例。
- **文書品質**: 実装契約は一意に読めるが、例、一覧、参照、用語が誤解を生む。
- **要明文化**: 自然な解釈では両立するが、scope/phase を補わないと別実装を誘発する。
- **不成立**: 文書が明示的に別surface/phaseを扱っている、または報告側の読み違い。

## 結論

重複を除く最終結果は次のとおり。

| 区分                         | 件数 | 意味                                       |
| ---------------------------- | ---: | ------------------------------------------ |
| 確定した契約・論理不整合     |   17 | 実装前に正本を決めて修正すべき             |
| 確定した例示・文書品質の問題 |    6 | 実装契約は概ね明確だが文書修正が必要       |
| 要明文化                     |    9 | 現状でも両立可能だが誤実装防止の追記を推奨 |
| 不成立となった代表的主張     |    8 | 統合findingには採用しない                  |

3レポートを単純に合算してはいけない。Codex は最も広く重要な問題を見つけているが「確定20件」は過大で、一部は明文化候補または根拠の読み違いである。Grok は Phase 3 overview と詳細文書の差をよく拾っているが、permission の `may`、optional field、public view と entry point を矛盾として扱った箇所がある。Claude は列番号例や未参照assetなど精度の高い独自指摘を持つ一方、SVG/cache の重大な不整合を見落とし、trivia capability を整合済みとした結論は妥当でない。

## 対応状況

この表にないfindingは未対応である。

| ID   | 状況     | 対応日     | 概要                                            |
| ---- | -------- | ---------- | ----------------------------------------------- |
| V-01 | 対応済み | 2026-07-10 | 2つのBinary AST SVGを現行v0.1 wire layoutへ更新 |
| V-02 | 対応済み | 2026-07-10 | Parser APIをtyped `Result`契約へ変更            |
| V-03 | 対応済み | 2026-07-10 | SemanticModelをfact-only constructionへ統一     |
| V-04 | 対応済み | 2026-07-10 | Trivia collection capabilityをresultに保持      |
| V-05 | 対応済み | 2026-07-10 | Cache keyへversion/options/exact sourceを追加   |
| V-06 | 対応済み | 2026-07-10 | Cache valueがSourceStore ownerを保持            |
| V-07 | 対応済み | 2026-07-10 | Formatterをragged matcher row対応へ変更         |
| V-08 | 対応済み | 2026-07-10 | Document IR SVGでCLI framingを分離              |
| V-09 | 対応済み | 2026-07-10 | CLI operational errorの二層配置を共通化         |
| V-10 | 対応済み | 2026-07-10 | Snapshot kind accessorをnumeric契約へ統一       |
| V-11 | 対応済み | 2026-07-10 | Version detailsをmajor/minor objectへ統一       |
| V-12 | 対応済み | 2026-07-10 | Unpaired surrogateをTypeError契約へ統一         |
| V-13 | 対応済み | 2026-07-10 | Stale editor requestをsilent no-opへ固定        |
| V-14 | 対応済み | 2026-07-10 | formatSnapshotのsourceを必須へ統一              |
| V-15 | 対応済み | 2026-07-10 | v0.x SyntaxKind追加を0.N draft bumpへ統一       |
| V-16 | 対応済み | 2026-07-10 | Agentのdocs slug依存を初期契約から削除          |
| V-17 | 対応済み | 2026-07-10 | Recommendedを低ノイズ・有用性基準へ統一         |

## 確定した契約・論理不整合

### V-01: Binary AST の2つのSVGが現行 v0.1 wire layout と不一致

- 対応状況: **対応済み（2026-07-10）**
- 正本: `design/003-ox-mf2-phase-2-binary-ast-snapshot-design.md:301-319,387-426`、changelog `:14-15`
- 修正前の不一致: 図は 28 byte header + padding、TokenRecord 32 bytes を示していた。
- 対応内容: `design/assets/003-ox-mf2-binary-ast-format-layout.svg:38-40,65-67,97-98` を Header 32B / TokenRecord 36B に更新し、header padding blockを削除した。
- 対応内容: `design/assets/003-ox-mf2-wire-layout.svg:20-21,59-67` のbyte 28..32を `reserved_tail: u32` とし、section tableがbyte 32から直ちに始まることを明示した。
- 判定: **確定**。Codex C-01 を採用。Claude の「wire format は整合」という確認は prose/changelog に限れば正しいが、参照図まで含めた結論としては不完全。
- 検証: XML parse成功。Playwrightのlight/dark表示で見切れ・矢印/文字重なりなし。両図ともviewBox外テキスト0件、box overflow 0件。

### V-02: Parser API signature が文書内の API error を返せない

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: parser entry pointは`ParseResult` / `ParseSessionResult`を直接返す一方、root欠落やresource overflowをparser diagnosticではないAPI errorと規定していた。
- 確定契約: `parse_source`、`parse_message`、`parse_source_session`は`Result<_, ParseError>`、`parse_batch`は`Result<BatchParseResult, BatchParseError>`を返す。
- エラー境界: MF2構文エラーは従来どおりdiagnosticsを持つ`Ok`。fatal failureだけを`SourceTooLarge`、`InvalidSourceId`、`ResourceLimit`、`MissingRoot`として`Err`にする。batchは`input_index`と下位`ParseError`を返し、partial resultを公開しない。
- 対応内容: `ParseErrorCode`を`4000..4999`に割り当て、Rust/TypeScriptのコード名対応、domain判定、設計appendixを同期した。
- 対応内容: snapshot convenience APIはfatal parse errorを既存の対応する`SnapshotWriteError`へ変換し、formatter・tests・benchmark harnessを新しい返却型へ移行した。
- 判定: **確定・解消済み**。
- 検証: Rust parser/formatter tests成功、parser doctest成功、benchmark harnessの全target `cargo check`成功、変更対象の`vp check`成功、`vp test`は110件成功・8件skip。

### V-03: SemanticModel construction/diagnostic ownership に旧契約が残る

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: 002とSemanticModel SVGはconstruction時のsyntax-adjacent checksとmodel-owned diagnosticsを残し、parser diagnosticsの有無にかかわらず`parse_semantic = true`だけで構築するよう読めた。008/012はconstructionをfact collection、validationをdiagnostic productionとし、parser diagnostics時はconstructionをskipする。
- 確定契約: 008/012を正本とし、SemanticModel constructionはparser diagnosticsが空の場合だけ実行する。SemanticModelはsemantic factsだけを所有し、semantic diagnosticsは独立した`validate_semantics(model)`境界が別に返す。
- 対応内容: 002のpipeline、non-responsibility、construction record一覧をfact-only契約へ更新した。
- 対応内容: `design/assets/002-ox-mf2-semantic-model-design.svg`からbuilder checksとmodel-owned diagnosticsを削除し、diagnostic-free gateと独立validation layerを明示した。
- 対応内容: Rustの`SemanticModel.diagnostics`を削除し、owned/session/batch materialisationのsemantic loweringをparser diagnosticsが空の場合に限定した。
- 判定: **確定・解消済み**。
- 検証: parser tests成功。SVGはXML parse成功、Playwrightのlight/dark表示で見切れ・重なりなし、viewBox外テキスト0件、box overflow 0件。
- 補正: Codex C-03 が根拠にした `001-ox-mf2-initial-architecture.svg` は、実際には CST から Formatter へ直接分岐しており、この点の根拠にはならない。

### V-04: 空の Trivia section が未収集triviaまで能力証明してしまう

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: Rust SnapshotWriterは`collect_trivia=false` / `include_trivia=true`でも空Trivia sectionを出し、未収集状態を「収集済みだが0件」と同じcapability markerにしていた。一方、bindingsは同じoption組合せを`TypeError`として拒否していた。
- 確定契約: `ParseResult` / `ParseSessionResult`が`trivia_collected`を保持する。`include_trivia=true`にはこの能力証明が必要で、未収集ならsnapshotを生成しない。`include_trivia=false`は収集状態にかかわらず有効。
- 対応内容: 全parser materialisation pathで`trivia_collected`を設定し、owned/session/batchを含む全snapshot入口で検証する。
- 対応内容: `SnapshotWriteError::TriviaNotCollected`とstable code `SnapshotWriteTriviaNotCollected = 2014`をRust/TypeScriptへ追加した。
- 対応内容: 002、003、error-code appendix、format changelogを新しいcapability契約へ同期した。既存のTypeScript bindings validationは変更不要だった。
- 判定: **確定・解消済み**。
- 検証: uncollected resultが空Trivia sectionを生成せずcode 2014で失敗するround-trip test、error-code mirror test、snapshot compatibility testが成功。

### V-05: Parse artifact cache key が結果を一意にしない

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: cache keyがnamespace/message/source hashだけで、結果を変えるParseOptionsとparser versionをhit判定に使わなかった。また「異なるsourceは衝突しない」という要件と、exact source比較なしでxxhashを許可する方針が両立しなかった。
- 確定契約: complete keyはnamespace、message id、parser version、normalized result-affecting options（`recovery`、`parse_semantic`、`collect_trivia`）、source fingerprintから成る。
- Source fingerprint契約: hashはMap検索の高速化だけに使い、key equalityはexact source bytesも比較する。hash一致・bytes不一致はmissであり、artifactを再利用しない。
- 対応内容: cache自身がrequestからkeyを構築するskeletonへ変更し、callerがsource/options/versionと食い違うprebuilt keyを渡せないようにした。V-02後の`Result`返却も反映した。
- 対応内容: invalidation invariant、one-parse invariant、hash選定open questionをcomplete keyとcollision test前提へ更新した。
- 判定: **確定・解消済み**。このcacheはdesign noteであり実装crateはまだ存在しないため、今回の変更対象は設計とskeletonのみ。

### V-06: Cache が owner を破棄した SourceId を保持する

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: cache valueはtemporary SourceStoreが割り当てたSourceIdだけを保存してownerを破棄し、その数値IDをfresh SourceStoreで再利用できると説明していた。CST・diagnostics・source/line indexの参照整合性が偶然のID再割当てに依存した。
- 確定契約: `CachedParse`は`Arc<SourceStore>`と完全な`ParseResult`を一体で所有する。result内の全SourceIdは必ず同じcached ownerへ解決する。
- 対応内容: 分解したCST/diagnostics/semantic/source/source_id fieldsを`{ sources, result }`へ置換し、parser API拡張時にもresult capabilityを失わない形にした。
- 対応内容: skeletonはper-entry SourceStoreとParseResultを同じArc artifactへ移動する。consumer例と禁止事項を追加し、fresh storeへの付け替え、numeric idだけの永続化、暗黙のID再利用を禁止した。
- 将来複数entryを一つのstoreへ統合する場合は、全SourceId-bearing recordの明示的remapが必要である。
- 判定: **確定・解消済み**。V-05と同じくdesign noteのみで、実装crateはまだ存在しない。

### V-07: Formatter IR が semantic arity error を grammar invariant と扱う

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: Formatter IRはrow key countとselector countの一致をgrammar invariantとし、不一致を`internal_error`にした。012ではarbitrary key countを構文上受理し、`variant-key-arity-mismatch` semantic diagnosticとしていたため、parser diagnosticsがない通常formatter入力からinternal errorへ到達できた。
- 確定契約: formatterはsemantic validationを実行せず、Data Model Errorをformatter operational errorへ変換しない。ragged rowも全source keyを順序どおり保持して安全に整形し、semantic diagnosticは独立validation layerが所有する。
- Alignment契約: column数は`max(selector count, maximum row key count)`。key不足rowはsource keyを合成せず空列分だけpaddingし、余分なkeyは削除せず追加列として扱う。
- 対応内容: `intlify_format`の固定arity checkを削除し、可変column幅計算と不足列paddingを実装した。011のIR invariantと007のformatter ruleを同期した。
- 対応内容: key不足・超過を同時に含むstandard/preserve fixtureとsnapshot-backed API testを追加した。
- 判定: **確定・解消済み**。
- 検証: `cargo test -p intlify_format --tests`成功（API 14件、fixture harness 1件）。formatted outputの再parseとidempotencyもfixture harnessで成功。

### V-08: Document IR SVG だけが message-level output に final LF を追加する

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: Document IR SVGだけがrenderer resultを`exactly one final LF`とし、007/011のmessage-level outputはunframedでfinal LFをCLI framingだけが付与する契約と衝突した。
- 確定契約: Document rendererは`unframed, no implicit LF`のmessage textを返す。CLI/file framingはmessage-level API外の任意境界で、file/stdout出力時にexactly one final LFを追加する。
- 対応内容: `design/assets/011-ox-mf2-document-ir.svg`のrenderer panelを拡張し、formatted message result、optional CLI/file framing、render errorを別領域に分離した。
- 判定: **確定・解消済み**。Rust rendererは既に暗黙final LFを追加していなかったためコード変更は不要。
- 検証: XML parse成功。Playwrightのlight/dark表示で見切れ・矢印/文字重なりなし。viewBox外テキスト0件、box overflow 0件。

### V-09: Operational error の格納先が Phase 3A と 3B/3C で変わっている

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: 006はoperational errorsをtop-level `errors`だけに置く一方、007/008は同じPhase 3A envelopeに従うとしながらfile-specific errorを`results[].errors`へ置いた。
- 確定契約: global operational errorsはtop-level `errors`、一つのselected targetに属するoperational errorsはcommand-specific `results[].errors`へ置く。両者は同じerror shapeとstable string code namespaceを使い、同じerrorを重複格納しない。
- 集計契約: いずれかのerror arrayが非空なら`summary.status = "error"`。command-specific `errorCount`はtop-levelとtarget-localの合計である。diagnosticsはどちらにも混在させない。
- 対応内容: 006のenvelope field説明、status集計、error placement、code namespaceを二層契約へ更新し、Phase 3A placeholderはtargetがないためtop-levelのみと明記した。
- 判定: **確定・解消済み**。現行`fmt` CLI実装は既に二層配置と合計`errorCount`を実装していたためコード変更は不要。

### V-10: Snapshot kind accessor が public numeric と public symbolic の二契約を持つ

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: 004はnumeric const object/unionをpublic representationとする一方、007はpublic accessorがsymbolic nameを返しnumeric discriminantはinternalと規定していた。
- 確定契約: public node/token `kind()` accessorはsnapshot recordおよびshared `SyntaxKind` const objectと同じnumeric `SyntaxKind` unionを返す。stable symbolic displayが必要な場合は`syntaxKindName(kind)`を使い、numeric orderingから意味を推測しない。
- 対応内容: 007のSnapshotView requirementsをnumeric accessor契約へ更新した。`packages/ox-mf2-shared`の既存実装は既にこの契約を満たすため、コード変更およびformatter専用の第二accessor追加は不要。
- 判定: **確定・解消済み**。

### V-11: `invalid_snapshot` version details が wire version を表現できない

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: 007の`invalid_snapshot`例はversionをscalarで表したが、wire headerと互換性判定はmajor/minor pairであり、現行v0.x decoderはpairのexact matchを行う。
- 確定契約: `details.version`はsnapshot headerから読んだ`{ major, minor }`、`details.supportedVersions`はdecoderが受理する同shapeの配列とする。package release versionとは区別する。
- 対応内容: 007のerror details例と説明を更新した。decoder errorが実際のheader versionを保持し、formatterのN-API/WASM境界がversion objectとsupported version配列を返すようにした。
- 対応内容: formatter operational error detailsをstring mapからJSON value mapへ拡張し、既存のstring detailsを同じ公開形のままnested object/arrayも転送できるようにした。
- 判定: **確定・解消済み**。Codex C-13、Claude A-5を採用。
- 検証: parser/formatter/N-API/WASMのRust tests成功。両binding artifactを再構築後、`vp test`は26 files・120 testsすべて成功。変更対象の`vp check`と`git diff --check`も成功。

### V-12: Unpaired surrogate の error class/code が二重

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: 004と実装はbinding inputのunpaired surrogateをbuilt-in `TypeError`とする一方、error-code appendixは`SourceTextErrorCode`がこのfailureを扱うと規定していた。
- 確定契約: parser inputと`withSources()`はraw JavaScript input validation時にunpaired surrogateを`TypeError`で拒否する。`SourceTextUnpairedSurrogate = 3004`はnumeric compatibilityのため予約を維持するが、Phase 2ではemitせず`OxMf2SourceTextError`にもmapしない。
- 対応内容: 004とerror-code appendixへvalidation boundaryとreserved/non-emitted policyを明記し、Rust/TypeScriptのcode declaration commentも同期した。公開済みcodeの削除・再利用は行わない。
- 判定: **確定・解消済み**。
- 検証: source text validationを含む`vp test`は26 files・120 testsすべて成功。Rust/TypeScript error-code compatibility tests、変更対象の`vp check`、`git diff --check`も成功。

### V-13: Stale editor edit の挙動が決定済みと open question の両方にある

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: 009のnormative textはstale mapping/versionをno-opとした一方、open questionsはoperational errorとの選択を未決定としていた。同様にwhole-message formattingを初期方針としながら、smallest editとの選択もopenに戻していた。
- 確定契約: document versionまたはmessage mappingがstale、あるいはcontaining messageを特定不能なら、adapterはoperational errorではなくsilentにno editsを返す。初期editはminimal diffを計算せずcontaining message range全体を置換し、standalone `.mf2`ではdocument全体を置換する。
- 対応内容: 009から解決済みの2つのopen questionを削除し、protocol固有のversion比較方法だけをopenに残した。共通transport設計005も同じno-op/edit range契約へ同期した。
- 判定: **確定・解消済み**。
- 検証: 変更対象の`vp check`成功。`vp test`は26 files・120 testsすべて成功し、`git diff --check`も成功。

### V-14: `formatSnapshot` の `source` が optional と required

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: 005は`formatSnapshot(snapshot, source?, options?)`とsourceをoptionalにした一方、007は`source: string`を必須としsource-free modeではないと規定していた。
- 確定契約: `formatSnapshot(snapshot, source, options?)`と`checkSnapshot(snapshot, source, options?)`は全modeで完全なsource stringを必須とする。source slicing、diagnostic materialization、output comparison、可能なsnapshot/source consistency checkに使用し、preserve modeではsource-shape判断にも使用する。
- 対応内容: 005のsignatureと説明を007、formatter package README、現行N-API/WASM実装の必須source契約へ同期した。コード変更は不要。
- 判定: **確定・解消済み**。3レポートで共通していたfindingを採用。
- 検証: 変更対象の`vp check`成功。`vp test`は26 files・120 testsすべて成功し、`git diff --check`も成功。

### V-15: v0.x SyntaxKind 追加時の version bump policy が不一致

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: 002は新しいcore `SyntaxKind`のemitに一律major version changeを要求した一方、003とformat changelogはv0.xをdraft exact-match versionとして扱っていた。
- 確定契約: v0.xで新しいcore kindをemitする場合は次の`0.N` draft versionへ進め、writer、decoder、accessor、changelog、fixturesを同時更新する。v0.x decoderはexact matchを維持する。v1.0以降、既存decoderが解釈不能なcore kind追加はmajor bumpとし、後方互換なoptional dataだけをminor bump候補とする。
- 不変条件: 公開済み`SyntaxKind`番号はv0.xでも並べ替え・再利用しない。
- 対応内容: 002、003、format changelogのversion bump checklistを段階別policyへ同期した。現行wire version自体はv0.1のままでありコード変更は不要。
- 判定: **確定・解消済み**。
- 検証: snapshot compatibility guard 9 tests成功。変更対象の`vp check`、全120件の`vp test`、`git diff --check`も成功。

### V-16: Agent が必要とする docs slug の取得元が存在しない

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: 010は`linter-rules/index.md`をagentがdocs slugを得るentry pointとした一方、008はslugをinternal generated metadataとし、design pages、runtime metadata API、public docs URL、JSON `help`のいずれも公開lookup contractではないと規定していた。
- 確定契約: 初期agent integrationのmachine-facing keyはstable diagnostic code/configurable rule idとする。`design/linter-rules/index.md`はrepository development中のreader-facing説明・remediation参照に限定し、docs slug、public URL、diagnostic `help`、runtime metadata APIには依存しない。
- 対応内容: 010からdocs slug取得要件を削除し、将来公開する場合はlinter-owned public metadata/help contractを別途定義すると明記した。008の非公開境界と一致したためAPI実装は不要。
- 判定: **確定・解消済み**。Codex R-03、Grok C-015を採用。
- 検証: 変更対象の`vp check`成功。`vp test`は26 files・120 testsすべて成功し、`git diff --check`も成功。

### V-17: `recommended` の方針説明と実際の category が一致しない

- 対応状況: **対応済み（2026-07-10）**
- 修正前の不一致: 005はrecommendedをcorrectness diagnostics中心と説明した一方、008のrecommended 2 rulesはともに`best-practice`で、唯一のconfigurable `correctness` ruleはcontext依存のためoff/非recommendedだった。
- 確定契約: recommended membershipはrule categoryではなく、message-levelで広く有用かつ低ノイズかで決める。現行best-practice 2 rulesは`warn`でrecommended、`no-undeclared-variable`はcorrectnessでもopt-inとする。parser/semantic correctness diagnosticsはconfigurable presetと独立してpipeline規約どおり有効にする。
- 対応内容: 005のpreset目的と0.x evolution policyを008の実際のmetadata/preset構成へ同期した。rule category、default severity、membershipおよび実装変更は不要。
- 判定: **確定・解消済み**。Codex C-20、Grok C-010を採用。
- 検証: 変更対象の`vp check`成功。`vp test`は26 files・120 testsすべて成功し、`git diff --check`も成功。

## 確定した例示・文書品質の問題

### E-01: Text reporter の列番号例が one-based 規則と不一致

- `design/008-ox-mf2-phase-3c-linter-design.md:535` は one-based display column。
- `:537-543` の `.input {$count...}` は `$` が1-based column 9だが、headerは `2:8`。
- 判定: **妥当**。Claude A-2の独自かつ正確な指摘。
- 推奨: `2:9` に直すか、例のcaret対象を明示する。

### E-02: Foundation のtext pipelineだけが formatterをSemanticModel経由に見せる

- `design/001-ox-mf2-toolchain-foundation.md:69-75` の直線的text図は CST → SemanticModel/SemanticView → formatter と読める。
- 直後 `:77` と `design/011-ox-mf2-formatter-ir-design.md:47-57` は formatter を CST/SnapshotView から直接分岐させる。
- 判定: **文書品質の問題**。Grok C-007、Codex C-19の核は妥当。
- 補正: `assets/001-ox-mf2-initial-architecture.svg` はCSTからFormatterへ正しく分岐済み。

### E-03: Phase 2 section 列挙で diagnostics の optionality が読めない

- `design/001-ox-mf2-toolchain-foundation.md:351` は optional を trivia/source text/extended data にだけ掛けたように読める。
- `design/003-ox-mf2-phase-2-binary-ast-snapshot-design.md:482` では diagnostics/diagnostic labels もoptional。
- 判定: **妥当な表現上の欠陥**。Claude B-1を採用。

### E-04: Semantic catalog の単独例が別の独立診断も発生させる

- `design/012-ox-mf2-parser-semantic-validation-design.md:338-381` の arity/missing-fallback/duplicate-variant 例は selector が未宣言・未注釈。
- 同文書 `:324-332` の規則では `missing-selector-annotation` も発生する。
- 判定: **妥当な例示上の欠陥**。例が「この診断だけ」と明記してはいないためcontract contradictionではない。
- 推奨: annotated input declarationを足すか、併発診断を明記する。

### E-05: Foundation のbenchmark aggregate名と詳細名の対応がない

- `design/001-ox-mf2-toolchain-foundation.md:225,227` の `snapshot_accessor_traversal` / `binding_call` に対し、003/004は `traverse_*` / `*_binding` に分解する。
- `lower_semantic` / `e2e_lint` にはcompatibility alias説明があるが、上記aggregate名にはない。
- 判定: **妥当なtraceability gap**。Claude B-6を採用するが、benchmark contractの矛盾ではない。

### E-06: 未参照SVGが1件ある

- `design/assets/003-ox-mf2-language-bindings.svg` は `design/**/*.md` から参照されない。
- 他の28 SVGはMarkdownから参照される。
- 判定: **妥当**。Claude B-8を採用。
- 推奨: 使用する、obsoleteとして削除する、または意図的な未参照assetと注記する。

## 要明文化: 両立可能だが誤読しやすい項目

### Q-01: Phase 1 SourceId map と rootごとのSourceRecord

- `design/003-ox-mf2-phase-2-binary-ast-snapshot-design.md:217` の単純な SourceId map は、`:636-638` と changelog `:57` の1 root=1 SourceRecordと緊張する。
- ただしmapをrootごとにclear/replaceする実装なら両立するため、Codex C-05の「表現不可能」は言い過ぎ。
- 推奨: `(root occurrence, Phase1 SourceId)` mapまたはroot-local mapと明記する。

### Q-02: `.editorconfig` を読むphase

- `design/005-ox-mf2-phase-3-tooling-transport-design.md:183` は将来的なformatter方針としてreadを要求し、007は初期実装では読まずoption導入後に有効化するとする。
- 005が「初期」と限定していないため論理的には両立する。Codex/GrokのHigh判定は過大、Claude B-5の評価が最も適切。
- 推奨: 005に「対応option導入後」を追記する。

### Q-03: Diagnostic shape の `binding result objects` のscope

- `design/008-ox-mf2-phase-3c-linter-design.md:154` はfmt/lint reportersとbinding result objectsがJSON shapeを共有すると書く。
- 文書scope上はlint N-API/WASM bindingsを指すと読め、007はformatter programmatic APIがparser `DiagnosticView`を使うと明示する。
- よってCodex C-11/Grok C-006の「二契約」は確定しないが、`linter binding result objects` と限定すべき。

### Q-04: `format-preserving first` と default standard

- foundationのfirstをlossless informationの設計優先度と読めば、製品default standardと両立する。
- preserveもstandard local rulesを適用するため、「original representation as much as possible」は制約付きの表現として読める。
- Grok C-003/C-016、Codex R-04は明文化候補としてのみ採用。

### Q-05: Linter の common syntax input

- 005のSnapshotViewは将来を含むpublic syntax foundation、初期Phase 3Cはsource + construction-time CstViewと読めば両立する。
- `common syntax input` が初期実入力にも見えるため、initial exceptionを明記する価値はある。

### Q-06: `errorCount` のsurface別意味

- 008はCLI operational countとprogrammatic diagnostic countを意図的に区別しており、内部矛盾ではない。
- 005のshared contractはその差を説明しない。また008の「Phase 3A meaning」は006に `errorCount` 定義がない。
- 推奨: CLI/programmaticの表を分け、Phase 3A参照を修正する。

### Q-07: Preserve mode の comment/literal wording

- 005の `may preserve` はexclusiveなmode差を意味せず、007が両modeでliteral spellingをrewriteしないことと両立する。
- MF2にcomment syntaxがないため `comment/trivia` のcomment部分は不要で誤解を招く。
- Grok C-008は確定矛盾ではなく用語整理として採用。

### Q-08: N-API platform target matrix

- parser packageはlinux-arm64-muslを含む7例、formatterは6例。lint例はnon-exhaustive。
- `existing label style/model` は命名モデルを指し、同一support setを要求していないため矛盾ではない。
- productごとのsupport差が意図的かだけを明記するとよい。Claude B-2はこの範囲で妥当。

### Q-09: Ignored stdin のbyte framing

- 通常stdinはread/write framing、ignored stdinは「original stdin source」を返す明示的special caseと読める。
- Codex R-02の矛盾判定は採用しないが、BOM/CRLFを含むbyte fixtureでraw passthroughを固定すると安全。

## 不成立となった代表的主張

| ID | 主張 | 判定理由 |
| --- | --- | --- |
| N-01 | 005のparallel許可と007/008のinitial sequentialが矛盾 | `may` はpermissionであり必須でない。詳細文書はfuture parallelも許す。 |
| N-02 | 005のoptional UTF-16 positionと008/009が矛盾 | 005はCLI/editor consumer向けoptional derived dataで、exact binding schemaを008へ委譲している。 |
| N-03 | Binary ASTをpublic inputとしつつ主APIがsourceなのは矛盾 | public syntax viewの形式とentry pointの引数は別境界。005自身が両方を区別する。 |
| N-04 | 005がlint JSONとformatter programmatic bindingに同一schemaを要求 | 該当文はlinter節でRust/N-API/WASM lint entry pointsを指す。formatter bindingまで広げていない。 |
| N-05 | `vp` と `vpr` の併用が契約矛盾 | 両CLIは実在し、root `package.json` も `vpr check/test/build` を使用する。表記方針の統一余地に留まる。 |
| N-06 | Node.js 22+ と `>=22.12.0` が矛盾 | 別packageの要件であり、`>=22.12.0` は22+の部分集合。 |
| N-07 | `result.sources` がusually input orderという表現が矛盾 | 弱い主張は常に順序一致する実装とも両立し、wire formatはshared SourceRecordも許す。 |
| N-08 | initial architecture SVGがformatterをSemanticModel経由にしている | SVGはCSTからFormatterへ直接branchしている。Codex C-03のこの根拠だけが誤り。 |

## 各レポートの総評

| レポート | 妥当性評価 |
| --- | --- |
| `design-contradiction-review-codex.md` | 最も広い。SVG、cache、error domain、wire/public APIの重要問題を多数発見した。一方「確定20件」には要明文化が混じり、C-03のinitial architecture解釈は誤り。C-03/C-18など重複もあるため、件数はそのまま採用しない。 |
| `design-contradiction-review-grok.md` | Phase 3 overviewの古い文言を整理する材料として有用。source引数、semantic旧記述、formatter pipeline、recommended、docs slugは妥当。parallel、UTF-16、public AST/source API、lint JSON schemaはover-classification。 |
| `design-contradiction-review-claud.md` | 列番号例、semantic旧記述、version例、optional section表現、compound examples、未参照assetの指摘は精度が高い。`vp`/`vpr`、Node versionは矛盾でない。SVG/cache/trivia capabilityを十分検証しておらず、「深刻な矛盾なし」という総括は採用できない。 |

## 原レポート項目の対応表

### Codex

| 原ID | 最終判定                                         |
| ---- | ------------------------------------------------ |
| C-01 | V-01                                             |
| C-02 | V-02                                             |
| C-03 | V-03。ただしinitial architecture SVGの根拠はN-08 |
| C-04 | V-04                                             |
| C-05 | Q-01へ格下げ                                     |
| C-06 | V-05                                             |
| C-07 | V-06                                             |
| C-08 | V-07                                             |
| C-09 | V-08                                             |
| C-10 | V-09                                             |
| C-11 | Q-03へ格下げ                                     |
| C-12 | V-10                                             |
| C-13 | V-11                                             |
| C-14 | V-12                                             |
| C-15 | V-13                                             |
| C-16 | V-14                                             |
| C-17 | Q-02へ格下げ                                     |
| C-18 | V-03へ統合                                       |
| C-19 | E-02                                             |
| C-20 | V-17                                             |
| R-01 | V-15へ昇格                                       |
| R-02 | Q-09。矛盾としては不採用                         |
| R-03 | V-16へ昇格                                       |
| R-04 | Q-04                                             |
| R-05 | Q-05                                             |
| R-06 | Q-06                                             |

### Grok

| 原ID  | 原タイトルの要約           | 最終判定     |
| ----- | -------------------------- | ------------ |
| C-001 | `formatSnapshot` source    | V-14         |
| C-002 | `.editorconfig`            | Q-02へ格下げ |
| C-003 | preserve-first/default     | Q-04へ格下げ |
| C-004 | parallelism                | N-01、不採用 |
| C-005 | syntax-adjacent validation | V-03         |
| C-006 | diagnostic shape           | Q-03へ格下げ |
| C-007 | formatter pipeline         | E-02         |
| C-008 | preserve対象               | Q-07へ格下げ |
| C-009 | linter input               | Q-05へ格下げ |
| C-010 | recommended/correctness    | V-17         |
| C-011 | errorCount                 | Q-06へ格下げ |
| C-012 | UTF-16 field               | N-02、不採用 |
| C-013 | public AST/source API      | N-03、不採用 |
| C-014 | lint JSON schema           | N-04、不採用 |
| C-015 | docs slug                  | V-16         |
| C-016 | preserve wording           | Q-04へ統合   |

### Claude (`claud.md`)

| 原ID | 最終判定                    |
| ---- | --------------------------- |
| A-1  | V-14                        |
| A-2  | E-01                        |
| A-3  | V-03                        |
| A-4  | N-05。style統一候補に留まる |
| A-5  | V-11                        |
| B-1  | E-03                        |
| B-2  | Q-08                        |
| B-3  | N-06、不採用                |
| B-4  | N-07、不採用                |
| B-5  | Q-02                        |
| B-6  | E-05                        |
| B-7  | E-04                        |
| B-8  | E-06                        |

## 推奨修正順

1. **互換性・データ完全性**: V-01、V-04、V-05、V-06、V-10、V-11、V-15
2. **API failure/ownership**: V-02、V-03、V-07、V-12、V-14
3. **CLI/editor machine contract**: V-08、V-09、V-13、V-16
4. **方針・例示・navigation**: V-17、E-01〜E-06
5. **誤実装防止の明文化**: Q-01〜Q-09

## 検証メモ

- 3レポートの全55見出しを対応表へ収録した。
- `design/` のMarkdownは26件、SVGは29件である。
- 未参照SVGは `design/assets/003-ox-mf2-language-bindings.svg` の1件だった。
- `vp` と `vpr` はどちらも現在のworkspaceで利用可能で、root package scriptsにも両表記が存在することを確認した。
- 4レポートをrepository標準へ整形後、リポジトリ全体の`vp check`は247 filesのformat、119 filesのlint/type checkに成功した。
- `vp test` はtest files 26件、tests 120件すべて成功した。
- `cargo test -p ox_mf2_parser -p intlify_format -p intlify_format_napi -p intlify_format_wasm --tests`、parser doctest、benchmark harnessの`cargo check --all-targets`が成功した。
- 本レポートは設計文書間の整合性評価であり、未実装コードの挙動を推測してfinding化していない。
