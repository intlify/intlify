# design/ 設計ドキュメント矛盾レビュー

- **Date**: 2026-07-10
- **Reviewer**: Grok (this investigation)
- **Scope**: `design/**/*.md`（`design/assets/` の図は関連箇所のみ）
- **観点**: 矛盾（文書間または文書内の論理的衝突）のみ
- **非対象**: 単なる未決定・Open Questions・明示的 defer の整合、実装と設計の差分、文体ゆれ

## サマリー

| Severity |  Count |
| -------- | -----: |
| high     |      4 |
| medium   |     10 |
| low      |      2 |
| **合計** | **16** |

**最も多く衝突している層**: Phase 3 横断の概要文書 `005` と、詳細契約 `007` / `008` の間。 **セマンティック検証の所有権・7 つの core semantic code・crate/package 名** は概ね整合している。

**推奨する正本の向き（修正時）**:

| 領域                              | 正本に寄せる文書                   |
| --------------------------------- | ---------------------------------- |
| セマンティック検証                | `012`（catalog / spans / cascade） |
| Linter 製品契約                   | `008`                              |
| Formatter 製品契約                | `007` / `011`                      |
| Binary AST wire                   | `003` + changelog                  |
| Phase 2 パーサ binding            | `004`                              |
| Tooling foundation / CLI envelope | `006`                              |

---

## Findings

### C-001 — high

**Title**: `formatSnapshot` の `source` が任意か必須か

|  |  |
| --- | --- |
| **Docs** | `005` vs `007` |
| **005** | `formatSnapshot(snapshot, source?, options?)`（`source` に `?`） |
| **007** | `formatSnapshot(snapshot, source: string, …)` / 「`source` is required for snapshot-backed formatting」 |

**Why**: 公開 API の引数契約が食い違う。005 は省略を型上許し、007 は preserve・source slice・診断・位置変換のため常時必須。

**Resolution**: 005 を 007 に合わせ、`source` を必須にする。

---

### C-002 — high

**Title**: `.editorconfig` を読むかどうか（Phase 3 初期契約）

|          |                                                                               |
| -------- | ----------------------------------------------------------------------------- |
| **Docs** | `005` vs `007`                                                                |
| **005**  | 「The formatter **should read** `.editorconfig` as formatter-only fallback…」 |
| **007**  | Non-goals: 初期は読まない。`mode` 以外の style option 導入後に fallback       |

**Why**: 初期実装の必須要件が逆。

**Resolution**: 005 を「現状は読まない／style option 導入後に fallback」と 007 に合わせる。

---

### C-003 — high

**Title**: 「format-preserving first」対「standard 既定」

|  |  |
| --- | --- |
| **Docs** | `001` vs `007` |
| **001** | 合意決定として `format-preserving first`。preserve = 「original representation as much as possible」 |
| **007** | 既定 `mode: "standard"`。preserve も local spacing / indent / matcher table を正規化する shape-sensitive pretty |

**Why**: foundation の製品方針と Phase 3B の既定・preserve 意味が反対方向。

**Resolution**: 001 を「trivia を残す foundation 方針」と「製品既定は standard」に分離し、preserve の意味を 007 に揃える。

---

### C-004 — high

**Title**: Phase 3 初期の並列実行可否（fmt / lint）

|          |                                                                  |
| -------- | ---------------------------------------------------------------- |
| **Docs** | `005` vs `007` / `008`                                           |
| **005**  | 「CLI **may** format / lint multiple files in parallel」         |
| **007**  | Phase 3B **initially** processes selected files **sequentially** |
| **008**  | Phase 3C processes selected files **sequentially**；並列は将来   |

**Why**: 005 は Phase 3 時点で並列を許可。製品詳細は初期を逐次固定。

**Resolution**: 005 を「初期 sequential；将来 parallel（出力順は不変）」に更新。

---

### C-005 — medium

**Title**: 002 に残る「syntax-adjacent」診断境界と 012 の validation 所有

|  |  |
| --- | --- |
| **Docs** | `002` vs `012` / `008` |
| **002** | Parser non-responsibilities: 「duplicate declaration policy **beyond** syntax-adjacent cases」；construction: 「detect syntax-adjacent duplicates or missing semantic anchors」 |
| **012 / 008** | construction は fact 収集のみ（validation diagnostics を出さない）。core semantic はすべて `validate_semantics` |

**Why**: 002 の更新段落は 012 に寄せている一方、古い「syntax-adjacent 検出」が残り、実装者が construction で診断を出すと誤読しうる。

**Resolution**: 002 から syntax-adjacent detect 文言を削除／「fact 収集のみ、診断は 012」に書き換え。関連 SVG も合わせて整理。

---

### C-006 — medium

**Title**: 「共有 diagnostic shape」の主張とパーサ / formatter programmatic shape の食い違い

|  |  |
| --- | --- |
| **Docs** | `008` vs `004` / `007` |
| **008** | category + kebab-case `code` + string severity。JSON は「fmt and lint reporters **and binding result objects**」で共有 |
| **004** | `DiagnosticView`: `category` なし、`code`/`severity` は numeric enum |
| **007** | formatter N-API/WASM は **parser packages と同じ** diagnostic JS shape を再利用 |

**Why**: 008 が product shape を「すべての binding」に広げすぎている。実際は tooling JSON / lint binding と parser/fmt programmatic で二系統。

**Resolution**: 008 を境界ごとに書き分ける（tooling/lint vs parser/fmt programmatic）。007 の境界説明を 008 からも参照。

---

### C-007 — medium

**Title**: フォーマッタ入力パイプライン（SemanticModel 経由 vs CST / SnapshotView）

|  |  |
| --- | --- |
| **Docs** | `001` 内 + `007` / `011` |
| **001** 図 | `lossless CST -> SemanticModel / SemanticView -> linter / **formatter** / compiler` |
| **001** 本文 | 「formatter primarily uses **CST**, tokens, and trivia」 |
| **007 / 011** | `SnapshotView`/CST → Layout IR → Document IR。SemanticModel はフォーマット入力に使わない |

**Why**: 001 内の図と本文が衝突し、製品設計とも衝突。

**Resolution**: 001 の図を分岐にする（formatter: CST/SnapshotView；linter/compiler: SemanticModel）。

---

### C-008 — medium

**Title**: preserve モードが保持する対象の記述不一致

|  |  |
| --- | --- |
| **Docs** | `005` vs `007` / `011` |
| **005** | preserve は「quote or literal spelling, and **comment**/trivia placement」を保持しうる |
| **007** | quote/literal/escape spelling は **両モード**で rewrite しない；preserve の主対象は single/multi-line と blank-line grouping |
| **005 / 007** 他節 | MF2 に line/block comment はなく、comment-like directive も非対応 |

**Why**: (1) comment 保持は「comment なし」と矛盾。(2) literal spelling を preserve 専用とする記述は 007 と矛盾。

**Resolution**: 005 の preserve 説明を 007 に合わせる。

---

### C-009 — medium

**Title**: リンタの「common syntax input」が SnapshotView か source + CstView か

|  |  |
| --- | --- |
| **Docs** | `005` SnapshotView 節 vs `005` 後段 / `001` / `008` |
| **005** | SnapshotView は formatter, **linter**, LSP… の common syntax input |
| **008 / 005 後段** | 初期 public API は `lintMessage(source)`；内部は `CstView` + `SemanticModel`；`lintSnapshot` は deferred |

**Why**: 「linter の common input = SnapshotView」と「初期は source re-parse + construction-time CstView」が両立しない。

**Resolution**: 005 を製品ごとに分ける（formatter: SnapshotView 公開入力可；初期 linter: source + 内部 CstView/SemanticModel）。

---

### C-010 — medium

**Title**: `recommended` が「correctness 中心」と書かれているが実体は best-practice

|  |  |
| --- | --- |
| **Docs** | `005` vs `008` / `linter-rules/index.md` |
| **005** | recommended は「broadly useful **correctness** diagnostics」中心 |
| **008** | recommended: `no-unused-declaration`, `no-duplicate-attribute`（いずれも `best-practice` / `warn`） |
| **008** | 唯一の `correctness` ルール `no-undeclared-variable` は default `off` / recommended 外 |

**Why**: カテゴリ語と preset 内容が逆向き。

**Resolution**: 005 を「broadly useful diagnostics（現状 best-practice）」に言い換えるか、008 の category/recommended 方針を 005 に合わせて見直す。

---

### C-011 — medium

**Title**: 共有結果の `errorCount` / `warningCount` 意味の曖昧さ

|  |  |
| --- | --- |
| **Docs** | `005` vs `008` |
| **005** | 共有 contract の例として `errorCount` / `warningCount` |
| **008 CLI** | `errorCount` = **operational errors**；診断数は `diagnosticErrorCount` / `diagnosticWarningCount` |
| **008 programmatic** | `errorCount` / `warningCount` = **診断 severity 集計**（operational は `ok: false`） |

**Why**: 同じフィールド名が CLI と programmatic で意味が違い、005 はその二義を示していない。

**Resolution**: 005 に CLI envelope と programmatic `LintResult` の集計フィールド分離を明記する。

---

### C-012 — medium

**Title**: 診断オブジェクトに UTF-16 位置を含めるか

|          |                                                                          |
| -------- | ------------------------------------------------------------------------ |
| **Docs** | `005` Diagnostic Result Contract vs `008` / `009`                        |
| **005**  | 共有 contract に「optional derived line/column or **UTF-16** positions」 |
| **008**  | 診断 shape に UTF-16 range を **追加しない**；adapter で変換             |
| **009**  | UTF-16 は editor adapter 境界                                            |

**Why**: 005 は共有 diagnostic result に UTF-16 を含め得ると読める。008/009 は shape から排除。

**Note**: 008 の `location` は **UTF-8 byte column** の line/column であり、UTF-16 ではない。line/column 自体は 008 にある。

**Resolution**: 005 を「canonical は UTF-8 span + 任意の line/column `location`。UTF-16 は LSP/editor adapter のみ」に書き換え。

---

### C-013 — medium

**Title**: 001 の「formatter public AST input = Binary AST」と 007 の主 API が source であること

|  |  |
| --- | --- |
| **Docs** | `001` / `005` vs `007` |
| **001 / 005** | Phase 2 以降、formatter の public AST input は Binary AST decoder/accessor view |
| **007** | 主 API は `formatMessage(source)`（内部 parse）。`formatSnapshot` は advanced 経路 |

**Why**: 「公開入力は Binary AST」と読むと、主製品 API が source-backed であることと衝突しやすい。005 後段は source 主・snapshot 副と書いているが、前段の断定と緊張する。

**Resolution**: 「公開 **syntax view** は SnapshotView 互換」と「公開 **entry point** は source-first、snapshot は reuse path」を明示的に分離する。

---

### C-014 — medium

**Title**: 005 の lint JSON が「Rust, N-API, WASM と同じ diagnostic schema」と断定

|  |  |
| --- | --- |
| **Docs** | `005` vs `007` / `004` / `008` |
| **005** | 「`json` should use the same diagnostic schema exposed by Rust, N-API, and WASM entry points」 |
| **実体** | lint の CLI/N-API/WASM は 008 shape（category + kebab-case）。fmt programmatic は 004 系 numeric `DiagnosticView`。CLI fmt は 008 shape |

**Why**: 「同じ schema」が製品横断の単一 shape と読めるが、実際は lint product と parser/fmt product で違う。

**Resolution**: 005 を「lint product 内で共有」「fmt CLI reporter は 008 shape、fmt programmatic は parser diagnostic shape」と限定。

---

### C-015 — low

**Title**: agent 向け `docs slug` の出所

|  |  |
| --- | --- |
| **Docs** | `010` vs `008` / `linter-rules/index.md` |
| **010** | agent は `linter-rules/index.md` を rule descriptions / **docs slugs** / remediation の entry point にする |
| **008** | docs slug は rule id から生成する **internal metadata** で、`design/linter-rules/*.md` path ではない |
| **linter-rules** | design-time 専用であり public runtime / generated docs の正本ではない |

**Why**: agent が design path を slug と誤用しうる。

**Resolution**: 010 から「docs slugs」を外し、slug が必要なら 008 の rule metadata（rule id 由来）を参照する。

---

### C-016 — low

**Title**: 001 の「preserve the original representation as much as possible」の表現が 007 の preserve より強い

|  |  |
| --- | --- |
| **Docs** | `001` vs `007` |
| **001** | preserve = 原表現を可能な限り保持 |
| **007** | preserve でも local spacing / indent / matcher table / 生成改行 LF を正規化。minimal-diff ではない |

**Why**: C-003 と関連する表現強度の差。単独でも読者誤誘導になる。

**Resolution**: C-003 と同時に 001 の preserve 定義を 007 の shape-sensitive 定義へ置換。

---

## 矛盾なしと確認した領域

| 領域 | 結果 |
| --- | --- |
| Semantic validation 所有権（parser owns, lint consumes） | 001 / 002（更新部）/ 008 / 012 で一致 |
| Core semantic code 7 種の一覧 | 002 例示 / 005 / 008 / 012 / `linter-rules/index.md` で一致 |
| SemanticModel は facts のみ（diagnostics 非所有） | 002 / 008 / 012 で一致 |
| Semantic diagnostics を `ParseResult.diagnostics` / Binary AST diagnostics に載せない | 002 / 003 / 008 / 012 で一致 |
| Configurable rules: `no-unused-declaration`, `no-duplicate-attribute`, `no-undeclared-variable` | 008 と `linter-rules/*` で metadata 一致 |
| attribute 重複 = lint、option 重複 = semantic | 意図的境界として文書化済み |
| package / crate 名（`ox_mf2_parser`, `intlify_{cli,format,lint}`, `@intlify/*`） | 001 / 004 / 005 / 006 / 007 / 008 で一致 |
| API error code range（appendix）と string operational codes の分離 | appendix / 004 / 006 / 007 / 008 で一致 |
| Binary AST v0.1 layout / optional sections / semantic 非同梱 | 003 + changelog で一致 |
| 無効構文では format しない（strict diagnostics） | 005 / 007 / 011 で一致 |
| 初期 linter は source-backed；cache / `lintSnapshot` は future | 008 / cache note / 005 後段で一致 |
| resource/catalog は message-level core の上位 adapter | 005 / 008 / 009 / cache note で一致 |
| 009 / 010 の fix API 非提供、style fix は formatter 委譲 | 007 / 008 と一致 |
| 006 Phase 3A reserved commands と 007/008 実装の段階差 | 矛盾ではなく phase 進化 |

---

## 修正優先度（推奨）

1. **C-001 / C-002 / C-004** — API 署名・設定挙動・実行モデル（実装直結）
2. **C-003 / C-007 / C-009 / C-013** — foundation / Phase 3 概要の入力モデル・製品方針
3. **C-006 / C-011 / C-012 / C-014** — diagnostic / summary shape の境界明示
4. **C-005 / C-008 / C-010** — 残存文言と preset 語彙
5. **C-015 / C-016** — 読者誤誘導の軽い修正

---

## レビュー方法メモ

- `design/*.md` と `design/linter-rules/*.md` を横断読解
- 矛盾ホットスポット（ownership、diagnostic codes/shape、formatter input、並列性、config、package 名、phase 境界）を重点照合
- 明示 defer・Open Question・「詳細は別文書」委譲で一貫しているものは findings から除外
- 詳細製品契約（007/008/012）を、古い横断概要（001/005）より優先して「どちらを正本に寄せるか」を判断
- セマンティック/診断、formatter/snapshot/phase、linter/rules/naming の 3 系統で並行照合し、原典を再確認して統合

---

## 参照ファイル一覧

- `design/001-ox-mf2-toolchain-foundation.md`
- `design/002-ox-mf2-phase-1-rust-parser-design.md`
- `design/003-ox-mf2-phase-2-binary-ast-snapshot-design.md`
- `design/003-ox-mf2-binary-ast-format-changelog.md`
- `design/004-ox-mf2-phase-2-language-bindings-design.md`
- `design/005-ox-mf2-phase-3-tooling-transport-design.md`
- `design/006-ox-mf2-phase-3a-tooling-foundation-design.md`
- `design/007-ox-mf2-phase-3b-formatter-design.md`
- `design/008-ox-mf2-phase-3c-linter-design.md`
- `design/009-ox-mf2-phase-3d-lsp-editor-design.md`
- `design/010-ox-mf2-phase-3e-agent-integration-design.md`
- `design/011-ox-mf2-formatter-ir-design.md`
- `design/012-ox-mf2-parser-semantic-validation-design.md`
- `design/appendix-ox-mf2-error-code.md`
- `design/ox-mf2-parse-artifact-cache.md`
- `design/linter-rules/*`
