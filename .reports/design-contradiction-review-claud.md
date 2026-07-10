# design/ 設計ドキュメント矛盾チェックレポート

- 対象: `design/` 配下の設計ドキュメント一式(メイン14本、`linter-rules/` 11本、appendix、`ox-mf2-parse-artifact-cache.md`、`assets/` 参照整合)
- 観点: 矛盾がないか(ドキュメント間およびドキュメント内)
- 実施日: 2026-07-10

---

## A. 明確な矛盾(修正推奨)

### A-1. `formatSnapshot` の `source` 引数: 省略可能 vs 必須(005 vs 007)

- `005-ox-mf2-phase-3-tooling-transport-design.md:136` では `formatSnapshot(snapshot, source?, options?)` と **`source` を省略可能**として記載。
- `007-ox-mf2-phase-3b-formatter-design.md:135-136, 180-186` では Rust / TypeScript とも `source` は**必須**。`007:205` に「`source` is required for snapshot-backed formatting … not a source-free formatting mode」と明記。
- `005:273` の `lintSnapshot(snapshot, source?, options?)` スケッチも同様に `source?` 表記(こちらは将来APIだが、008 は「formatter のスナップショット入力制約を踏襲する」としているため、同じ齟齬を引き継ぐ可能性がある)。

### A-2. テキストレポーターの列番号規約と例の食い違い(008 内部矛盾)

- `008-ox-mf2-phase-3c-linter-design.md:535` は「テキストレポーターの列は **1始まりの表示列**」と規定。
- 直後の例 `008:539` は `[messages/foo.mf2:2:8]`。対象の `$count` は `.input {` の直後にあるため 0始まりバイト列で 8、**1始まり表示列なら 9** になるはず。
- この「8」は同ファイルの JSON 例(`008:163` の `location.column: 8`、0始まり UTF-8 バイト列)と同じ値であり、JSON の座標系をテキスト例に流用したように見える。

### A-3. SemanticModel 構築時の重複検出に関する古い記述(002 vs 008/012)

- `002-ox-mf2-phase-1-rust-parser-design.md:308` は SemanticModel 構築の仕事として「**detect syntax-adjacent duplicates** or missing semantic anchors」を列挙。
- `002:65` のパーサー非責務は「duplicate declaration policy **beyond syntax-adjacent cases**」(= syntax-adjacent な重複はパーサー側、と読める)。
- `002:1127` も「duplicate semantic key **that cannot be decided from syntax alone**」をセマンティック側の例として挙げている(= 構文から決められる重複は別、と読める)。
- 一方、確定済みの契約では:
  - `008:94`「現在の SemanticModel 構築は検証診断を出さずにレコードを収集するだけ」
  - `012` は duplicate-declaration 系を**すべて `validate_semantics` 所管**と規定
  - `002:986`(ゼロ診断保証)自身も「duplicate declarations はセマンティック診断であり、パーサー診断ゼロで成立する」と記載
- 002 のこれらの箇所は初期設計の名残で、012 の確定契約(構築 = 事実収集のみ、検出 = `validate_semantics`)と食い違っている。002 内部でも記述間に緊張がある。

### A-4. ツールチェーンコマンド表記の揺れ: `vp` vs `vpr`

- `006-ox-mf2-phase-3a-tooling-foundation-design.md:554` は**同一文内**で `vp run cli#bench:startup` と `vpr check` / `vpr test` を併用。
- `003-ox-mf2-binary-ast-format-changelog.md:92` と `007:1142` も `vpr check` / `vpr test` を使用。
- `006:501-505` およびプロジェクト指示(CLAUDE.md)のコマンドは `vp`(`vp check` / `vp test` / `vp run …`)。`vpr` がエイリアスだとしてもドキュメント内に定義がなく、表記が不統一。

### A-5. `invalid_snapshot` の details 例がスナップショットのバージョン設計と不整合(007 vs 003)

- `007:328-333` の例: `"version": 3, "supportedVersions": [1, 2]`(単一整数バージョン・複数バージョンサポート)。
- `003` の実フォーマットは `major_version` / `minor_version`(現行 v0.1)で、v0.x は**完全一致マッチ**(サポートは 0.1 のみ)。
- 例示とはいえ、形式(単一整数)も値(1, 2 をサポート)も実設計と噛み合わない。

---

## B. 軽微な不整合・誤読を招く記述

### B-1. 001 の Phase 2 セクション列挙で diagnostics の任意性が読めない

`001-ox-mf2-toolchain-foundation.md:351` は「…optional trivia, **diagnostics, diagnostic labels, string table**, optional source text data, optional extended data」と、trivia / source text / extended のみ optional と読める書き方。しかし `003:482` と changelog では diagnostics / diagnostic labels も **optional セクション**(string table は core で正しい)。

### B-2. N-API プラットフォームパッケージ集合の不一致(linux-arm64-musl)

- パーサー `@intlify/ox-mf2-napi`(`004:34-42`)は **7ターゲット**(`linux-arm64-musl` 含む)。
- `@intlify/format-napi`(`007:768-775`)は「using the existing label style」としながら **6ターゲット**(`linux-arm64-musl` なし)。
- `008:38` の lint-napi 例も 6(non-exhaustive と明記あり)。CLI ネイティブ(`006`)も arm64-musl は将来候補。
- 意図的な差の可能性はあるが、「既存モデルに従う」と述べつつターゲット集合が黙って異なるため、意図の明記がないと矛盾に見える。

### B-3. Node.js バージョン基準の不揃い

`004:48`「N-API package: Node.js 22 or later」vs `006:448` `engines.node: >=22.12.0`。パッケージが異なるため直接の矛盾ではないが、基準の粒度が揃っていない。

### B-4. 004 の「result.sources は usually 入力順」

`004:397`「In v0.1 default writer output this **usually** matches input order」。しかし changelog(`003-ox-mf2-binary-ast-format-changelog.md:57`)で v0.1 ライターは**必ず**入力 root ごとに 1 SourceRecord を root 順に出すため、v0.1 では常に入力順になる。将来仕様の含みとしても v0.1 の記述としては弱く、誤読を招く。

### B-5. `.editorconfig` の扱い(005 vs 007)

`005:183`「formatter は `.editorconfig` を読む**べき**」に対し、`007`(non-goals および `007:687`)は「初期実装では読まない(`mode` 以外のオプションがないため)」。007 側は理由付きの明示的な先送りなので設計上の破綻ではないが、005 側に「消費できるオプションが存在してから」という条件が書かれていないため、単独で読むと食い違う。

### B-6. ベンチマークフェーズ名のドリフト(001 vs 003/004)

`001` の `snapshot_accessor_traversal` / `binding_call` は、正式名では `traverse_nodes` / `traverse_tokens` / `traverse_diagnostics`(003)、`parse_message_binding` / `parse_batch_binding` / `decode_snapshot_binding`(004)に細分化されている。001 は `lower_semantic` と `e2e_lint` についてのみ「互換名 / レガシー別名」と注記しており、上記 2 つには対応付けの注記がない。

### B-7. 012 カタログの例が単独診断にならない

`012` の variant-key-arity-mismatch(`012:339-348`)、missing-fallback-variant(`012:358-368`)、duplicate-variant(`012:376-381`)の例は selector が未宣言 / 未注釈のため、仕様どおり全独立違反を報告すると **missing-selector-annotation も併発**する。linter-rules 側の対応ページは注釈付き宣言を追加して診断を分離済み。012 の例は説明用だが、期待診断が併記されていないため誤読の余地がある(「Fixtures and Validation」節のカスケードフィクスチャは期待診断明記済みで問題なし)。

### B-8. 未参照アセット

`assets/003-ox-mf2-language-bindings.svg` はどの md からも参照されていない(004 は `003-ox-mf2-binary-ast-binding-architecture.svg` を使用)。他の 28 個の SVG 参照はすべて実在ファイルと一致。

---

## C. 疑って確認した結果、整合していた主要ポイント

- **ワイヤフォーマット**: 003 の全レコードサイズ(Header 32 / SectionRecord 20 / Root 16 / Source 32 / Node 24 / Edge 8 / Token 36 / Trivia 16 / Diagnostic 28 / Label 16 / StringOffset 8 / SourceTextRef 12 / ExtHeader 8)はフィールド合計と一致し、changelog・002 の Phase 1 サイズ予算(内部 Token 28 → ワイヤ 36 への線形展開)とも整合。
- **エラーコード体系**: DecodeErrorCode の連番(1000..1035、InvalidSpan=1035)は appendix のレンジ(1000..1999)と changelog に一致。SnapshotWrite=2000s / SourceText=3000s / Init=10000s / BindingValidation=11000s も 004・appendix 間で一致。CLI の文字列コードと数値 `OxMf2ErrorCode` の分離も全文書で一貫。
- **セマンティック診断 7 コード**(duplicate-declaration ほか)は 002 / 005 / 008 / 012 / linter-rules/index で完全一致。設定可能ルール 3 件のメタデータ(カテゴリ・デフォルト severity・recommended 所属)も 008 と linter-rules の表・個別ページで一致。
- **リントパイプライン**(parser→semantic→rules の段階排他、パーサー / セマンティック診断は常に error、`ok:true` にパーサー診断を含む)、**exit code**(0/1/2、優先 2>1>0、warn は `--max-warnings` 超過時のみ 1)、`--quiet` の計数動作は 005 / 006 / 007 / 008 / 009 で一貫。
- **スナップショットの capability proof**(include\_\* 有効時の空セクション emit)と、フォーマッターの preserve モードの trivia 必須 / standard は不要、`invalid_snapshot`(missing_capability)→ IR 構築後は `internal_error` という境界は 003 / 007 / 011 で一致(`internal_error` の phase 名 5 種も 007 と 011 で一致)。
- **ファイルフレーミング**(BOM + 最終改行 1 個)、隠しファイル / VCS ディレクトリ除外、ignore 優先順(root `.gitignore` → `--ignore-path` → `ignorePatterns`)は 007 と 008 で共有・一致。
- WASM `init()` 状態機械(004 / 007 / 008)、config 契約(root-only 発見、JSON/JSONC、unknown field 拒否、`$schema` 許容)、バージョン `0.14.0-alpha.0`(006 / 007)、ドキュメント間リンク・アンカー参照もすべて整合。

---

## 総括

実装を壊すレベルの深刻な設計矛盾はない。ただし、以下の 3 件は下流実装者が誤った実装をし得る箇所なので修正を推奨する。

1. **A-1**: `formatSnapshot` の `source?`(005 のシグネチャスケッチを 007 の必須仕様に合わせる)
2. **A-2**: テキストレポーター列番号の例(`2:8` → `2:9`、または規約の再確認)
3. **A-3**: 002 の syntax-adjacent duplicate 検出の古い記述(012 の確定契約に合わせて整理)

A-4 / A-5 と B 群は表記・例示レベルの不整合。
