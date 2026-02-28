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
- **リスク**: nagaバージョンアップで出力形式が変わる可能性
- **対策**: nagaバージョン固定、または出力形式変更時のテスト失敗で検知

### 3.4 レイマーチパラメータ

- **ファイル**: `src/shaders/raymarch_template.wgsl:152`
- **問題**: `MAX_STEPS=256`, `MIN_DIST=0.001` がハードコード
- **影響**: 複雑なSDFでは調整が必要な場合あり
- **将来**: Uniform化を検討

---

## 4. 配布・運用

### 4.1 genmesh実行パス

- **ファイル**: `crates/sdf-baker/src/genmesh_runner.rs`
- **問題**: `genmesh_path` のデフォルトが `"genmesh"` でPATHに依存
- **推奨**: 配布時はより明示的なパス解決（実行ファイル同梱ディレクトリ等）

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
| 中 | `extract_user_functions()` のnagaバージョン依存を文書化 |
| 中 | genmesh実行パスの明示的解決 |
| 低 | レイマーチパラメータのUniform化 |
| 低 | half_to_float のC++20モダン化 |
