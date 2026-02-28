# sdf-baker 実装 TODO

## 概要

Rust CLI ツール `sdf-baker` を新規作成し、SDF シェーダ (WGSL/GLSL) から
GPU compute で距離場を評価、genmesh v1 仕様のブリックファイルを書き出し、
`genmesh` CLI を呼び出して STL メッシュを生成するまでの最短パイプラインを構築する。

## 参照ドキュメント

- 計画書: docs/plan.md (§5, §5.1, §5.2)
- genmesh 仕様: docs/spec/genmesh_v1.md
- スキーマ:
  - docs/schemas/manifest.v1.schema.json
  - docs/schemas/bricks-index.v1.schema.json
  - docs/schemas/report.v1.schema.json
- genmesh 実装: tools/genmesh/ (v1 完了済み)
- 既存 GUI アプリ: src/ (eframe + wgpu、プレビュー専用。本 TODO では触れない)

## ディレクトリ構成 (目標)

```
print-organizer/
  Cargo.toml                 ← workspace root に変換
  crates/
    sdf-baker/
      Cargo.toml             ← wgpu, serde, serde_json, sha2, bytemuck, anyhow, clap
      src/
        lib.rs               ← pub API (GUI からも利用可能)
        main.rs              ← CLI エントリポイント
        cli.rs               ← clap 引数定義
        gpu.rs               ← wgpu デバイス初期化 (ヘッドレス)
        compute.rs           ← compute pipeline: ブリック単位 SDF 評価
        shader_compose.rs    ← ユーザー sdf() + compute テンプレート結合
        bricks_writer.rs     ← manifest.json + bricks.index.json + bricks.bin 書き出し
        genmesh_runner.rs    ← genmesh 呼び出し + report.json 解析
        types.rs             ← BakeConfig, BrickMeta, BakeResult 等
      tests/
        test_bricks_writer.rs
        test_compute.rs
        test_e2e.rs
      shaders/
        compute_template.wgsl  ← compute dispatch テンプレート (WGSL 版)
  src/                         ← 既存 GUI アプリ (変更なし)
  tools/genmesh/               ← 既存 C++ CLI (変更なし)
```

## シェーダ合成の設計

ユーザーが `fn sdf(p: vec3<f32>) -> f32` (WGSL) を含むファイルを渡すと、
以下の compute テンプレートの `{{USER_SDF}}` 部分に埋め込む。

```wgsl
// --- compute_template.wgsl ---
struct Params {
    aabb_min: vec3<f32>,
    voxel_size: f32,
    brick_offset: vec3<u32>,  // (bx*B, by*B, bz*B)
    brick_size: u32,          // B (64)
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> output: array<f32>;

{{USER_SDF}}

@compute @workgroup_size(4, 4, 4)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let B = params.brick_size;
    if (gid.x >= B || gid.y >= B || gid.z >= B) { return; }

    let global_idx = vec3<u32>(
        params.brick_offset.x + gid.x,
        params.brick_offset.y + gid.y,
        params.brick_offset.z + gid.z,
    );

    let p = params.aabb_min + params.voxel_size * (vec3<f32>(global_idx) + vec3<f32>(0.5));
    let d = sdf(p);

    let idx = gid.x + B * (gid.y + B * gid.z);
    output[idx] = d;
}
```

GLSL 版 (R4 で追加):
- ユーザーは `float sdf(vec3 p)` を定義
- compute テンプレートは `#version 450` + `layout(local_size_x=4,...)` 形式
- `ShaderSource::Glsl { shader, stage: Compute, defines }` で wgpu に渡す

---

## Phase R0: ワークスペース化 + クレート骨格 ✅ (0f21e87)

### R0.1 Cargo workspace 変換
- ルート `Cargo.toml` に `[workspace]` セクションを追加
- `members = ["crates/sdf-baker"]`
- 既存 GUI アプリはルート package として残す (`[package]` と `[workspace]` 共存)
- `cargo build` が既存アプリ + sdf-baker 両方通ること
- Accept: `cargo build` がエラーなく完了

### R0.2 sdf-baker クレート作成
- `crates/sdf-baker/Cargo.toml`:
  - `wgpu` (compute 用、features: vulkan/dx12/metal)
  - `pollster` (async → sync ブリッジ、ヘッドレス GPU 用)
  - `bytemuck` (GPU バッファ ↔ Rust 型)
  - `clap` (derive, CLI パース)
  - `serde` + `serde_json` (manifest/index 書き出し)
  - `sha2` (ハッシュ計算)
  - `anyhow` (エラー処理)
  - `log` + `env_logger`
- `src/main.rs`: 最小の hello world
- `src/lib.rs`: 空の pub mod 宣言
- Accept: `cargo run -p sdf-baker` が "sdf-baker v0.1.0" を表示

### R0.3 CLI 引数定義 (cli.rs)
- clap derive で以下を定義:
  - `--shader <path>` (WGSL/GLSL ファイルパス。未指定時は内蔵 sphere)
  - `--out <dir>` (出力ディレクトリ、必須)
  - `--aabb-min <x,y,z>` (既定: 0,0,0)
  - `--aabb-size <x,y,z>` (既定: 64,64,64)
  - `--voxel-size <float>` (既定: 1.0)
  - `--brick-size <int>` (既定: 64、{32,64,128})
  - `--half-width <int>` (既定: 3)
  - `--iso <float>` (既定: 0.0)
  - `--adaptivity <float>` (既定: 0.0)
  - `--dtype <f32|f16>` (既定: f32)
  - `--genmesh-path <path>` (genmesh 実行ファイルのパス)
  - `--skip-genmesh` (ブリック出力のみで genmesh を呼ばない)
  - `--write-vdb` (genmesh に --write-vdb を渡す)
  - `--force` (出力上書き)
  - `--log-level <error|warn|info|debug>` (既定: info)
- Accept: `cargo run -p sdf-baker -- --help` がヘルプを表示

---

## Phase R1: GPU compute — 1 ブリック SDF 評価

### R1.1 ヘッドレス wgpu デバイス初期化 (gpu.rs)
- `wgpu::Instance` → `request_adapter` → `request_device`
- 電源優先 (`PowerPreference::HighPerformance`)
- `features`: compute shader が動く最小限
- `limits`: `max_storage_buffer_binding_size` ≥ 64^3 * 4 = 1MB
- エラー時は anyhow でメッセージ付き失敗
- Accept: ユニットテストで device が取得でき、adapter 名がログに出る

### R1.2 Compute テンプレート + シェーダ合成 (shader_compose.rs)
- `shaders/compute_template.wgsl` をファイルまたは `include_str!` で保持
- `compose_wgsl(user_code: &str) -> String`:
  - テンプレートの `{{USER_SDF}}` を `user_code` で置換
  - 返却文字列を wgpu に渡せる完全な WGSL
- 内蔵 sphere SDF (フォールバック用):
  ```wgsl
  fn sdf(p: vec3<f32>) -> f32 {
      return length(p - vec3<f32>(32.0, 32.0, 32.0)) - 25.6;
  }
  ```
- Accept: compose_wgsl に sphere SDF を渡してコンパイルが通る (wgpu validation)

### R1.3 Compute パイプライン実行 (compute.rs)
- `bake_brick(device, queue, shader_module, config, brick_coord) -> Vec<f32>`
- 手順:
  1. Params uniform バッファ作成 (aabb_min, voxel_size, brick_offset, brick_size)
  2. Output storage バッファ作成 (B^3 * 4 bytes)
  3. Bind group 作成
  4. Compute pipeline 作成
  5. Command encoder → dispatch (B/4, B/4, B/4)
  6. output バッファを staging バッファにコピー → map_async → 読み取り
- `bake_all_bricks(device, queue, shader_module, config) -> Vec<BrickResult>`
  - 全ブリック座標 (bx,by,bz) を列挙
  - 各ブリックで bake_brick を呼ぶ
  - background_value_mm との比較で全セル background のブリックをスキップ (スパース)
- Accept: 内蔵 sphere SDF で 1 ブリック (64^3) の距離値が取得でき、
  中心付近で負、外周で正の値が得られる

### R1.4 R1 統合テスト
- `test_compute.rs`:
  - device 初期化 → sphere SDF → 1 ブリック bake
  - values[center_index] < 0 (内部)
  - values[corner_index] > 0 (外部)
  - 値の数が 64^3 = 262144
- Accept: `cargo test -p sdf-baker` が green

---

## Phase R2: ブリック書き出し (genmesh 仕様準拠)

### R2.1 型定義 (types.rs)
- `BakeConfig`: aabb_min, aabb_size, voxel_size, dims, brick_size,
  half_width_voxels, iso, adaptivity, dtype, background_value_mm
- `BrickResult`: bx, by, bz, values (Vec<f32>), is_background (bool)
- `BakeOutput`: config, bricks (Vec<BrickResult>), timing
- Accept: 型が定義され、serde Serialize が derive されている

### R2.2 manifest.json 書き出し (bricks_writer.rs)
- `write_manifest(dir, config) -> Result<PathBuf>`
- manifest.v1.schema.json の全必須フィールドを出力:
  - version: 1
  - coordinate_system: { handedness: "right", up_axis: "Y", front_axis: "+Z" }
  - units: "mm"
  - aabb_min, aabb_size, voxel_size, dims
  - sample_at: "voxel_center", axis_order: "x-fastest"
  - distance_sign: "negative_inside_positive_outside"
  - iso, adaptivity
  - narrow_band: { half_width_voxels }
  - brick: { size }
  - dtype: "f32" (v1 では f32 のみ生成)
  - background_value_mm
  - hashes: {} (後で SHA256 を埋める場合に備えて空オブジェクト)
- Accept: 出力 JSON が manifest.v1.schema.json に手動で照合して valid

### R2.3 bricks.bin + bricks.index.json 書き出し (bricks_writer.rs)
- `write_bricks(dir, config, bricks) -> Result<()>`
- bricks.bin:
  - background でないブリックのみ書き出し
  - f32 little-endian, x-fastest order (GPU 出力がそのまま)
  - offset_bytes を逐次計算
- bricks.index.json:
  - bricks-index.v1.schema.json 準拠
  - version, brick_size, dtype, axis_order, dims
  - bricks 配列: bx, by, bz, offset_bytes, payload_bytes, encoding: "raw"
- Accept: genmesh が読めるファイルが生成される (R3 で検証)

### R2.4 R2 テスト
- `test_bricks_writer.rs`:
  - ダミーの BrickResult → write_manifest + write_bricks
  - 出力ファイルが存在し、JSON が正しくパースできる
  - bricks.bin のサイズが bricks 数 * B^3 * 4
  - manifest の dims, voxel_size, aabb_size の整合
- Accept: `cargo test -p sdf-baker` が green

---

## Phase R3: genmesh 呼び出し + report 解析

### R3.1 genmesh 実行 (genmesh_runner.rs)
- `run_genmesh(config) -> Result<GenmeshResult>`:
  - `std::process::Command` で genmesh を実行
  - 引数: `--manifest <out>/manifest.json --in <out> --out <out> --force`
  - `--write-stl` (常に)
  - `--write-vdb` (オプション)
  - `--iso`, `--adaptivity` を渡す
  - stderr をキャプチャしてログ出力
  - exit code を確認 (0 以外はエラー)
- `GenmeshResult`:
  - exit_code: i32
  - stdout: String
  - stderr: String
  - report: Option<Report> (report.json をパースしたもの)
- Accept: genmesh が見つかり実行でき、exit code が返る

### R3.2 report.json パース (genmesh_runner.rs)
- `Report` 構造体 (serde Deserialize):
  - schema_version, status, stage
  - timing_ms (各フェーズ)
  - stats (triangle_count, vertex_count, etc.)
  - warnings, errors
- genmesh 実行後に `<out>/report.json` を読み取り、Report にデシリアライズ
- 結果をログに出力 (tri count, timing 等)
- Accept: sphere の report.json がパースでき、triangle_count > 0

### R3.3 E2E 統合 (main.rs)
- main.rs のパイプライン:
  1. CLI パース
  2. GPU デバイス初期化
  3. シェーダ読み込み (--shader 指定 or 内蔵 sphere)
  4. シェーダ合成 + compile
  5. 全ブリック bake
  6. ブリック書き出し (manifest + index + bin)
  7. genmesh 呼び出し (--skip-genmesh でスキップ可能)
  8. 結果サマリ表示
- Accept:
  ```
  cargo run -p sdf-baker -- --out out/ --genmesh-path tools/genmesh/build/Debug/genmesh.exe
  ```
  で `out/mesh.stl` + `out/report.json` が生成される

### R3.4 E2E テスト
- `test_e2e.rs`:
  - 内蔵 sphere → bake → write → genmesh (パスが通る環境のみ)
  - mesh.stl が存在し、サイズ > 0
  - report.json が存在し、status == "success"
  - triangle_count == 24672 (genmesh sphere baseline と一致)
- Accept: `cargo test -p sdf-baker` が green (genmesh が PATH にある環境)

---

## Phase R4: 外部シェーダ読み込み (WGSL + GLSL)

### R4.1 WGSL ファイル読み込み (shader_compose.rs)
- `load_shader(path) -> Result<(ShaderLang, String)>`
  - 拡張子で判別: `.wgsl` → WGSL, `.glsl` / `.frag` / `.comp` → GLSL
  - ファイル UTF-8 読み込み
  - 最低限のバリデーション: `sdf` 関数が存在するか文字列検索
    - WGSL: `fn sdf(` を含む
    - GLSL: `float sdf(` を含む
- Accept: WGSL ファイルを渡して compose → compile が通る

### R4.2 GLSL compute テンプレート + 合成
- `shaders/compute_template.glsl`:
  ```glsl
  #version 450
  layout(local_size_x = 4, local_size_y = 4, local_size_z = 4) in;

  layout(set = 0, binding = 0) uniform Params {
      vec3 aabb_min;
      float voxel_size;
      uvec3 brick_offset;
      uint brick_size;
  };
  layout(set = 0, binding = 1) buffer Output { float data[]; } output_buf;

  {{USER_SDF}}

  void main() {
      uvec3 gid = gl_GlobalInvocationID;
      uint B = brick_size;
      if (gid.x >= B || gid.y >= B || gid.z >= B) return;
      vec3 p = aabb_min + voxel_size * (vec3(brick_offset + gid) + vec3(0.5));
      float d = sdf(p);
      uint idx = gid.x + B * (gid.y + B * gid.z);
      output_buf.data[idx] = d;
  }
  ```
- `compose_glsl(user_code: &str) -> String`: テンプレート合成
- compute.rs: `ShaderSource::Glsl { shader, stage: Compute, defines }` 対応分岐
- Accept: GLSL の sdf ファイルを渡して sphere メッシュが生成される

### R4.3 R4 テスト
- WGSL 外部ファイル → bake → 値の妥当性
- GLSL 外部ファイル → bake → 値の妥当性
- 不正シェーダ (sdf 未定義) → エラーメッセージ
- Accept: `cargo test -p sdf-baker` が green

---

## Phase R5: マルチブリック対応 + 最適化

### R5.1 dims > 64 のグリッド分割
- `BakeConfig` から自動計算:
  - `bricks_per_axis = ceil(dims[i] / brick_size)`
  - 全 (bx,by,bz) 組み合わせを列挙
- ブリック数が多い場合の進捗表示 (stderr / log)
- Accept: `--aabb-size 128,128,128 --voxel-size 1.0 --brick-size 64` で
  2^3 = 8 ブリックが処理され、メッシュが生成される

### R5.2 スパース最適化
- background_value_mm のみのブリックを検出してスキップ
  - 全値が `abs(val - background_value_mm) < eps` なら background
- bricks.index.json から除外、bricks.bin にも書かない
- Accept: 球 SDF で角のブリックがスキップされ、ファイルサイズが小さくなる

### R5.3 パイプライン再利用
- compute pipeline を 1 回作成し、全ブリックで再利用
  (uniform バッファだけ毎回更新)
- Accept: 8 ブリック処理時間が pipeline 作成 8 回の場合より短い

### R5.4 R5 テスト
- 128^3 グリッド (8 ブリック) で sphere → genmesh → tri > 0
- スパースブリック数 < 全ブリック数
- Accept: `cargo test -p sdf-baker` が green

---

## 備考

### genmesh 実行パスの解決
- 開発中は `--genmesh-path` で明示指定
- 将来的には:
  - 環境変数 `GENMESH_PATH`
  - PATH 検索
  - build.rs で CMake 成果物を検出

### wgpu バックエンド
- Windows: Vulkan or DX12 (wgpu が自動選択)
- ヘッドレス (CI) でも compute shader は動作する (GPU ドライバ必要)
- GPU が無い環境では `wgpu::Backends::GL` フォールバックも検討 (R5 以降)

### 既存 GUI アプリとの統合 (将来)
- `sdf-baker` を lib として依存に追加
- GUI 側で `BakeConfig` を構築し `bake_all_bricks` → `run_genmesh` を呼ぶ
- プレビュー → パラメータ確認 → 生成 の UI フローに組み込む
