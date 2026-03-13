# コードレビュー注意点

最終レビュー日: 2026-03-01

## 1. コンパイル警告

| ファイル | 問題 | 状態 |
|----------|------|------|
| `src/app.rs:181` | `ConfigInfo::out` フィールドが未使用（dead_code警告） | ✅ 解消済（フィールド削除） |

**対応済**: フィールドを削除して警告解消

---

## 2. genmesh (C++)

### 2.1 型チェックの改善余地

- **ファイル**: `tools/genmesh/src/manifest.cpp`
- **箇所**: `require_field<T>` テンプレート（行73付近）
- **問題**: 型チェックがキャッチ任せ（コメント「type check via get_to would throw; we catch below」）
- **✅ 対応済**: `is_number_integer()` / `is_number()` / `is_string()` による明示的型チェックを追加（`<type_traits>` の `if constexpr` 使用）

### 2.2 half_to_float 実装

- **ファイル**: `tools/genmesh/src/bricks_data.cpp:21-48`
- **問題**: ソフトウェアf16→f32変換が正確だが冗長
- **推奨**: C++20以降なら `<bit>` + `<cstdint>` でより簡潔に書ける（将来リファクタ候補）

### 2.3 グローバル変数

- **箇所**: `min_log_level()` がグローバル変数
- **問題**: スレッドセーフ性要確認（現状シングルスレッドで問題なし）

---

## 3. GUI (Rust)

### 3.1 パイプライン再構築

- **ファイル**: `src/app.rs:361`
- **問題**: `render_pipeline` 再構築時、古いパイプラインの明示的ドロップがない
- **影響**: wgpuが自動管理するが、大きなシェーダでは注意

### 3.2 デバイス参照の取得

- **ファイル**: `src/app.rs:642-658`
- **問題**: `device.clone()` でArc参照を毎フレーム取得
- **推奨**: `Option<Arc<Device>>` をキャッシュするとよりクリーン

### 3.3 naga出力依存

- **ファイル**: `src/preview_compose.rs:62-95`
- **問題**: `extract_user_functions()` が naga出力の `fn main_1(` パターンに依存
- **なぜ必要か**: GLSLには頂点+フラグメントのマルチステージテンプレートがなく、nagaは完全なシェーダステージを要求するため、ダミー compute シェーダに包んで変換し、ユーザー関数だけ抽出する必要がある
- **リスク**: nagaバージョンアップで出力形式が変わる可能性
- **✅ 緩和策**: コードに詳細な理由とリスクを文書化済。nagaバージョンは wgpu 27 経由で固定。`test_compose_glsl_preview` / `test_glsl_sdf_to_wgsl_extracts_function` が形式変更時に検知

### 3.4 app.rs の肥大化

- **ファイル**: `src/app.rs` (~960行)
- **問題**: ベイクパイプライン・設定表示・カメラ・wgpuレンダリング・UI描画がすべて1ファイルに混在
- **分割方針**: `src/app.rs` → `src/app/` ディレクトリに分割

| モジュール | 責務 | 行数目安 |
|---|---|---|
| `mod.rs` | `MyApp` struct定義・`new()` 全体フロー・`load_config_file()`・`eframe::App` impl（UI描画）。`BakeStatus` もUI状態としてここに残す | ~400 |
| `bake.rs` | `BakeResult`, `spawn_bake()`, `run_bake_pipeline()` ※`BakeStatus`は含めない | ~170 |
| `config_info.rs` | `ConfigInfo` struct + `from_config()` | ~70 |
| `camera.rs` | `OrbitCamera` struct + `from_aabb()` / `position()` | ~40 |
| `renderer.rs` | GPU初期化補助 + `fallback_wgsl()`, `create_offscreen_texture()`, `render_to_texture()`, `rebuild_preview_pipeline()` | ~250 |

- **原則**: `bake.rs`, `config_info.rs`, `camera.rs` は `MyApp` に依存しない独立モジュール。
- **BakeStatus の配置**: `BakeStatus`(Idle/Running/Done) は UI 状態機械なので `mod.rs` に残す。`BakeResult` のみ `bake.rs` に配置。
- **renderer.rs の方式**: `impl MyApp` メソッドを別ファイルに分割する方式A を採用。`new()` 内の GPU 初期化補助（バッファ・テクスチャ生成等）も `renderer.rs` に寄せ、レンダリング責務の分散を避ける。独立 `Renderer` 構造体（方式B）は `render_to_texture` が 8 フィールドを参照するため引数が煩雑になる。将来 MyApp がさらに肥大化した場合に方式Bへ移行を検討。
- **段階的方針**: まず「見通し改善」を目的にファイル分割し、次段で「境界整理」（`load_config_file()` の抽出等）を検討する。
- **互換性**: `app.rs` → `app/mod.rs` 変更は `main.rs` の `mod app;` に互換。
- **ステータス**: ✅ 実装済み (3f8065e)

### 3.5 レイマーチパラメータ

- **ファイル**: `src/shaders/raymarch_template.wgsl:152`
- **問題**: `MAX_STEPS=256`, `MIN_DIST=0.001` がハードコード
- **影響**: 複雑なSDFでは調整が必要な場合あり
- **将来**: Uniform化を検討

---

## 4. 配布・運用

### 4.1 genmesh実行パス ✅

- **ファイル**: `crates/sdf-baker/src/config.rs` (`resolve_genmesh_path()`)
- **問題**: `genmesh_path` のデフォルトが `"genmesh"` でPATHに依存
- **対応**: 4段階の探索順を実装（`docs/spec/project-json.md` §1「`genmesh.path` 探索順」参照）
  1. project.json `genmesh.path` → 2. 環境変数 `PRINT_ORGANIZER_GENMESH` → 3. exe同梱ディレクトリ → 4. PATH
- **ステータス**: ✅ spec 策定済み・実装済み (a149324)

### 4.2 パラメータ編集UI

- **問題**: GUI側でvoxel_size/brick_sizeの編集UIがない
- **現状**: config jsonを直接編集する想定
- **将来**: 必要に応じてUI追加

---

## 5. 対応優先度

| 優先度 | 項目 |
|--------|------|
| 高 | `ConfigInfo::out` のdead_code警告を解消 | ✅ |
| 高 | genmesh `require_field<T>` 型チェック改善 | ✅ |
| 中 | `extract_user_functions()` のnagaバージョン依存を文書化 | ✅ |
| 中 | genmesh実行パスの明示的解決 | ✅ 実装済み (a149324) |
| 中 | app.rs 分割 (§3.4) | ✅ 実装済み (3f8065e) |
| 低 | レイマーチパラメータのUniform化 |
| 低 | half_to_float のC++20モダン化 |
