# シェーダ ホットリロード計画（草案）

対応タスク: plan.md Task D

## 1. 目的

シェーダファイル（`.wgsl` / `.glsl`）の保存を検知し、プレビューパイプラインを自動で再コンパイル・再描画する。
GLSL のコンパイルエラーを行番号付きで UI に表示し、エディタとの往復を高速化する。

## 2. 現状

| 項目 | 現在の仕組み |
|------|-------------|
| シェーダ読み込み | `load_config_file()` → `sdf_baker::shader_compose::load_shader()` → `rebuild_preview_pipeline()` |
| エラー表示 | `shader_error: Option<String>` → UI に黄色バナー1行 |
| 再読み込み | 手動（JSON を再度開く必要あり） |
| ファイル監視 | なし |

### 課題

1. **手動再読み込み**: シェーダを編集してもプレビューが更新されない
2. **エラー情報が乏しい**: naga のパースエラーは行番号・スパン情報を持つが、現在は `Display` フォーマットの1行メッセージしか表示していない
3. **GLSL エラーの二重変換問題**: GLSL → naga → WGSL 変換時のエラーはダミーラッパーの行番号を指すため、ユーザーの元ソースの行番号とずれる

## 3. 設計

### 3.1 ファイル監視

- **クレート**: `notify` (crates.io 最も成熟した FS watcher)
- **監視対象**: `MyApp.resolved_shader_path`（config JSON から解決された絶対パス。§3.5 参照）
- **イベント**: `Modify` / `Create` / `Rename` を受理（エディタの保存方式に依存しないため）。VS Code の atomic save は rename 系イベントになるため `Modify` のみでは不十分。
- **通知経路**: `notify::Watcher` → `mpsc::Sender<()>` → `MyApp` の UI ループで受信

```
┌──────────┐   Modify    ┌──────────┐  mpsc   ┌─────────┐
│ FS       │ ──────────> │ notify   │ ──────> │ MyApp   │
│ (shader) │             │ Watcher  │         │ update()│
└──────────┘             └──────────┘         └─────────┘
                                                   │
                                          load_shader()
                                          rebuild_preview_pipeline()
                                          request_repaint()
```

### 3.2 リロードフロー

```
1. update() で watcher_rx.try_recv() を確認
2. シェーダパスを再読み込み（load_shader）
3. compose_preview() で WGSL テンプレートに合成
4. 成功 → rebuild_preview_pipeline()、shader_error = None
5. 失敗 → shader_error にエラー情報を格納、前のパイプラインを維持
6. request_repaint()
```

### 3.3 デバウンス

- エディタは保存時に複数の write イベントを発火することがある（特に VS Code の atomic save）
- **方式**: 最後のイベントから 200ms 経過後に1回だけリロードを発火
- **実装**: `watcher_rx` 受信時に `Instant::now()` を記録し、`ctx.request_repaint_after(Duration::from_millis(200))` で次回 update を予約。`update()` 内で 200ms 経過を確認してからリロード実行。これにより静止状態でもデバウンス完了後に確実に update が呼ばれる。

### 3.4 Watcher のライフサイクル

| イベント | 動作 |
|---------|------|
| JSON 読み込み（`load_config_file`） | シェーダパスがあれば Watcher を開始（前の Watcher があれば drop） |
| 別の JSON を開く | 旧 Watcher を drop → 新しいシェーダパスで Watcher 再開始 |
| アプリ終了 | Watcher は `MyApp` の drop で自動停止 |

### 3.5 MyApp への追加フィールド

```rust
// ファイル監視
watcher: Option<notify::RecommendedWatcher>,
watcher_rx: Option<mpsc::Receiver<()>>,
pending_reload: Option<Instant>,  // デバウンス用
resolved_shader_path: Option<PathBuf>,  // 監視対象の実パス（ConfigInfo.shader は表示用文字列のため別管理）
```

## 4. GLSL エラー表示の改善

### 4.1 現状のエラーパス

```
GLSL ソース
  → naga::front::glsl::Frontend::parse()
    → Err(Vec<naga::front::glsl::Error>)
      → 各 Error は .meta (Span) を持つ
  → 現在: "{e}" で1行表示
```

### 4.2 改善方針

#### A. naga パースエラーから行番号を抽出

naga の GLSL パースエラー（`naga::front::glsl::Error`）は `meta: Span` を持ち、
ソースの byte offset を格納している。これをユーザーの GLSL ソースの行番号に変換する。

```rust
fn format_glsl_errors(
    errors: &[naga::front::glsl::Error],
    source: &str,  // ユーザーの元ソース（ラッパー除去済み）
    line_offset: u32,  // ダミーラッパーによるオフセット補正
) -> String {
    // Span.start → 行番号変換
    // line_offset 分を差し引いてユーザー行番号にマッピング
}
```

#### B. ダミーラッパーの行番号オフセット補正

GLSL はダミー compute ラッパーに包まれるため、naga が報告する行番号には
ラッパーのプリアンブル行数（現在3行: `#version`, `layout`, `buffer`）分のオフセットがある。

- ラッパーのプリアンブル行数を定数化（`GLSL_WRAPPER_PREAMBLE_LINES = 3`）
- naga の報告行番号から引いてユーザーの元ソース行番号を算出

#### C. WGSL 検証エラー（合成後のテンプレート）

WGSL テンプレートにユーザー SDF を埋め込んだ後の `validate_wgsl()` エラーは
テンプレートの行番号を返す。ユーザー SDF 挿入位置のオフセットを引けば
ユーザーの元行番号にできるが、精度に限界がある。

- **初期実装**: テンプレート行番号をそのまま表示し `(テンプレート内行番号)` とラベル付け
- **将来**: 挿入オフセットによる補正を検討

#### D. UI 表示の改善

現在の1行バナーを複数行のエラーパネルに拡張する。

```
⚠ Shader errors:
  Line 12: unknown type 'vec4'
  Line 25: expected ';', found '}'
```

- `shader_error: Option<String>` → `shader_errors: Vec<ShaderDiagnostic>` に変更
- `ShaderDiagnostic { line: Option<u32>, message: String, severity: DiagSeverity }`
- UI: `ScrollArea` 内に赤/黄で行番号付きリスト表示
- 行番号をクリックして外部エディタの対応行にジャンプ（将来: `opener` + VS Code URI）

## 5. 実装ステップ

### Step 1: `notify` 導入 + 基本ファイル監視

- `Cargo.toml` に `notify = "7"` 追加
- `MyApp` に watcher フィールド + `resolved_shader_path: Option<PathBuf>` 追加
- `load_config_file()` 内でシェーダパスを `resolved_shader_path` に保存し、Watcher 開始
- `update()` で `try_recv()` → `rebuild_preview_pipeline()` 呼び出し
- **Done when**: WGSL シェーダを VS Code で保存するとプレビューが自動更新される（atomic save 対応含む）

### Step 2: デバウンス

- `pending_reload: Option<Instant>` で 200ms デバウンス実装
- watcher イベント受信時に `ctx.request_repaint_after(200ms)` で次回 update を確実に発火
- **Done when**: VS Code の atomic save で二重リロードが発生しない。静止状態（ユーザー操作なし）でもデバウンス後にリロードが走る

### Step 3: `shader_compose` に structured diagnostics を追加

- 現状 `glsl_to_wgsl()` は naga の parse errors を `format!("{e}")` で文字列化し、`anyhow` に変換しているため Span 情報が失われる
- 行番号付き診断を実現するには、`shader_compose` 側で Span を保持したエラー型を返す API に変更が必要
- 新規エラー型の設計: `ShaderDiagnostic { line: Option<u32>, column: Option<u32>, message: String }` を `sdf-baker` 側に定義
- **Done when**: `glsl_to_wgsl()` または新 API が行番号付き診断情報を返す

### Step 4: GLSL / WGSL エラー行番号付き表示

- `shader_error: Option<String>` → `shader_errors: Vec<ShaderDiagnostic>` に型変更
- Step 3 の structured diagnostics を使い、naga パースエラーから行番号を抽出、ラッパーオフセット補正
- WGSL 検証エラーのテンプレート行番号抽出
- UI のエラーバナーを複数行表示に変更
- **Done when**: GLSL に構文エラーを入れるとユーザーの元ソースの行番号付きでエラー表示される。WGSL はテンプレート内行番号で表示（元ソース行番号への補正は将来検討）

## 6. 依存クレート

| クレート | 用途 | バージョン |
|---------|------|-----------|
| `notify` | ファイル監視 | 7.x |

## 7. リスク・注意点

- **naga エラー型の公開度**: naga の GLSL パースエラー型が `pub` でない場合、行番号抽出が困難。要調査。
- **Windows ファイルロック**: 一部エディタはファイルを排他ロックして書き込む。`notify` は `ReadDirectoryChangesW` を使うため影響しないが、シェーダ読み込み時のファイルロック競合に注意。
- **GLSL 行番号精度**: ダミーラッパーが単純なので現状のオフセット補正で十分だが、ラッパー構造を変更した場合は追従が必要。

## 8. 未決事項

- [ ] naga GLSL パースエラー型の Span アクセスが可能か確認
- [ ] デバウンス時間の最適値（200ms は仮）
- [ ] watcher の通知が macOS / Linux で同等に動くか（Windows 優先だが将来の移植性）

## 9. レビュー履歴

### 9.1 デバウンスと再描画 — ✅ 反映済み

- **指摘**: `pending_reload: Option<Instant>` だけでは 200ms 経過後の `update()` が保証されない。eframe は静止時にイベントループをスリープさせるため。
- **検証**: 妥当。現在の repaint ロジック（`needs_repaint || BakeStatus::Running` のときのみ `ctx.request_repaint()`）では watcher イベント後の再描画が保証されない。egui は `ctx.request_repaint_after(Duration)` API を提供している。
- **対応**: §3.3 と Step 2 に `request_repaint_after(200ms)` の使用を明記。

### 9.2 notify イベント種別 — ✅ 反映済み

- **指摘**: `EventKind::Modify` のみでは VS Code の atomic save（temp → rename）を検知できない。
- **検証**: 妥当。VS Code の atomic save は temp ファイルに書き込み後 rename するため、`Rename` イベントが発火する。`Modify` のみの監視では保存を検知できない。
- **対応**: §3.1 のイベント種別を `Modify` / `Create` / `Rename` に拡張。Step 1 の Done 条件に VS Code atomic save 対応を含めた。

### 9.3 監視対象パスの保持方法 — ✅ 反映済み

- **指摘**: `ConfigInfo.shader` は表示用文字列（組み込み SDF 時は `"(built-in sphere)"`）であり、watcher の入力として不適切。
- **検証**: 妥当。`ConfigInfo::from_config()` で `cfg_dir.join(s).display().to_string()` と変換されており、`PathBuf` ではなく表示用文字列。`load_config_file()` では `cfg_dir.join(shader_rel)` でパスを再構築しており、ConfigInfo とは別経路。
- **対応**: §3.5 に `resolved_shader_path: Option<PathBuf>` を追加。Step 1 に反映。

### 9.4 GLSL 診断 API の前提不足 — ✅ 反映済み

- **指摘**: `glsl_to_wgsl()` は `format!("{e}")` で naga の parse errors を文字列化しており、Span 情報が失われる。行番号付き診断には `shader_compose` 側の API 拡張が先に必要。
- **検証**: 妥当。`crates/sdf-baker/src/shader_compose.rs` で `.map_err(|parse_errors| { ... format!("{e}") ... anyhow::anyhow!(...) })` としており、Span メタデータは捨てられている。
- **対応**: Step 3 を「`shader_compose` に structured diagnostics を追加」に変更し、Step 4 (UI 表示) の前提とした。

### 9.5 実装順の整理案 — ✅ 反映済み

- **指摘**: ファイル監視と診断改善を切り分け、Step 3 を `shader_compose` API 拡張、Step 4 を UI 表示にすべき。
- **検証**: 妥当。Step 4 は Step 3 の structured diagnostics がないと実装できない依存関係がある。
- **対応**: §5 の実装ステップを提案通りの順序に再編成。
