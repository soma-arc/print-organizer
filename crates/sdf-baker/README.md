# sdf-baker

SDF シェーダ (WGSL / GLSL) を GPU compute で評価し、ブリック化された距離場データを書き出し、[genmesh](../../tools/genmesh/) を呼び出して STL メッシュを生成する CLI ツール。

## 概要

```
  .wgsl / .glsl         sdf-baker              genmesh
  fn sdf(p) → f32  ──►  GPU bake  ──►  bricks  ──►  mesh.stl
                                                      report.json
```

### パイプライン

1. GPU デバイス初期化 (wgpu, ヘッドレス)
2. シェーダ読み込み — 外部ファイル (`.wgsl` / `.glsl` / `.comp`) または内蔵球 SDF
3. シェーダ合成 — ユーザーの `sdf()` 関数を compute テンプレートに埋め込み
4. GPU compute — ブリック単位 (64³) で SDF 値を評価
5. スパース最適化 — 表面を含まないブリックをスキップ
6. ブリック書き出し — `manifest.json` + `bricks.bin` + `bricks.index.json`
7. genmesh 呼び出し — OpenVDB で等値面メッシュ化 → STL 出力

## 前提条件

| 要件 | 詳細 |
|------|------|
| **Rust** | edition 2024, stable |
| **GPU** | Vulkan / DX12 / Metal 対応 + ドライバ |
| **genmesh** | STL 生成時に必要 (`--skip-genmesh` で省略可) |

## ビルド

```bash
# ワークスペースルートから
cargo build -p sdf-baker

# テスト (61 tests — GPU 統合テスト含む)
cargo test -p sdf-baker
```

## 使い方

### 基本（内蔵球 SDF）

```bash
cargo run -p sdf-baker -- \
  --out output/ \
  --genmesh-path tools/genmesh/build/Debug/genmesh.exe \
  --force
```

### 外部 WGSL シェーダ

```bash
cargo run -p sdf-baker -- \
  --shader my_sdf.wgsl \
  --out output/ \
  --genmesh-path path/to/genmesh.exe
```

### 外部 GLSL シェーダ

```bash
cargo run -p sdf-baker -- \
  --shader my_sdf.glsl \
  --out output/ \
  --genmesh-path path/to/genmesh.exe
```

### ブリック書き出しのみ

```bash
cargo run -p sdf-baker -- --out output/ --skip-genmesh --force
```

## CLI オプション

| フラグ | 必須 | デフォルト | 説明 |
|--------|------|-----------|------|
| `--shader <path>` | — | 内蔵球 SDF | ユーザー SDF シェーダファイル (.wgsl / .glsl / .comp) |
| `--out <dir>` | ✔ | — | 出力ディレクトリ |
| `--aabb-min <x,y,z>` | — | `0,0,0` | AABB 最小角 |
| `--aabb-size <x,y,z>` | — | `64,64,64` | AABB 各軸サイズ |
| `--voxel-size <float>` | — | `1.0` | ボクセル辺長 (world units) |
| `--brick-size <int>` | — | `64` | ブリック辺長 (32 / 64 / 128) |
| `--half-width <int>` | — | `3` | ナローバンド半幅 (voxels) |
| `--iso <float>` | — | `0.0` | 等値面の値 |
| `--adaptivity <float>` | — | `0.0` | メッシュ簡略化 (0.0–1.0) |
| `--dtype <type>` | — | `f32` | 距離値のデータ型 (f32 / f16) |
| `--genmesh-path <path>` | — | `genmesh` | genmesh 実行ファイルのパス |
| `--skip-genmesh` | — | `false` | genmesh を呼ばずブリック出力のみ |
| `--write-vdb` | — | `false` | genmesh に `--write-vdb` を渡す |
| `--force` | — | `false` | 出力ディレクトリの上書き許可 |
| `--log-level <level>` | — | `info` | ログレベル (error / warn / info / debug) |

## シェーダの書き方

### WGSL

```wgsl
fn sdf(p: vec3<f32>) -> f32 {
    return length(p - vec3<f32>(32.0, 32.0, 32.0)) - 25.6;
}
```

### GLSL

```glsl
float sdf(vec3 p) {
    return length(p - vec3(32.0)) - 25.6;
}
```

ユーザーは `sdf()` 関数のみを定義する。compute dispatch のテンプレート (`@compute`, uniform, storage buffer) は sdf-baker が自動合成する。GLSL は naga 経由で WGSL に変換してから wgpu に渡される。

## 出力ファイル

| ファイル | 説明 |
|---------|------|
| `manifest.json` | グリッドメタデータ (SDF パラメータ、座標系、解像度) |
| `bricks.bin` | ブリック距離場データ (f32 LE, x-fastest) |
| `bricks.index.json` | ブリックオフセット/サイズのインデックス |
| `mesh.stl` | 三角形メッシュ (genmesh が生成) |
| `report.json` | 実行統計・タイミング・エラー (genmesh が生成) |

## プロジェクト構成

```
crates/sdf-baker/
├── Cargo.toml
├── src/
│   ├── lib.rs               # pub モジュール宣言
│   ├── main.rs              # CLI エントリポイント + E2E パイプライン
│   ├── cli.rs               # clap 引数定義 (16 オプション)
│   ├── gpu.rs               # wgpu ヘッドレスデバイス初期化
│   ├── shader_compose.rs    # シェーダ合成 (WGSL/GLSL) + naga 変換
│   ├── compute.rs           # GPU compute パイプライン + bake
│   ├── types.rs             # BakeConfig, ComputeParams, BrickResult
│   ├── bricks_writer.rs     # manifest + bricks.bin + index 書き出し
│   └── genmesh_runner.rs    # genmesh 呼び出し + report.json 解析
├── shaders/
│   ├── compute_template.wgsl  # WGSL compute テンプレート
│   └── compute_template.glsl  # GLSL compute テンプレート
├── tests/
│   ├── test_compute.rs        #  6 tests — GPU bake 検証
│   ├── test_bricks_writer.rs  #  4 tests — ファイル出力検証
│   ├── test_shader.rs         #  5 tests — WGSL/GLSL 外部シェーダ
│   ├── test_multi_brick.rs    #  7 tests — マルチブリック + スパース
│   └── test_e2e.rs            #  5 tests — genmesh 含む E2E
└── TODO.md                    # 実装フェーズ記録
```

## 仕様

JSON スキーマ:
- [manifest.v1.schema.json](../../docs/schemas/manifest.v1.schema.json)
- [bricks-index.v1.schema.json](../../docs/schemas/bricks-index.v1.schema.json)
- [report.v1.schema.json](../../docs/schemas/report.v1.schema.json)

## ライセンス

[MIT](../../LICENSE)
