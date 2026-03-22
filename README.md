# print-organizer

SDF（符号付き距離場）シェーダから 3D プリント用 STL メッシュを生成するパイプライン。

```
                          ┌──────────────┐
  .wgsl / .glsl  ──────►  │  sdf-baker   │  GPU compute で SDF 評価
  (fn sdf(p) → f32)       │  (Rust CLI)  │  + ブリック書き出し
                          └──────┬───────┘
                                 │ manifest.json
                                 │ bricks.bin
                                 │ bricks.index.json
                          ┌──────▼───────┐
                          │   genmesh    │  OpenVDB で等値面抽出
                          │  (C++ CLI)   │  + STL 書き出し
                          └──────┬───────┘
                                 │
                          mesh.stl + report.json
```

## コンポーネント

| コンポーネント | 言語 | 説明 |
|---------------|------|------|
| **print-organizer** (ルート) | Rust | GUI プレビューアプリ (eframe + wgpu) |
| [**sdf-baker**](crates/sdf-baker/) | Rust | SDF → GPU bake → genmesh → STL の CLI パイプライン |
| [**genmesh**](tools/genmesh/) | C++ | ブリック SDF データ → OpenVDB → STL メッシュ変換 CLI |

## クイックスタート

### 前提条件

- **Rust** (edition 2024, stable)
- **GPU** — Vulkan / DX12 / Metal 対応 GPU + ドライバ
- **genmesh** を使う場合: CMake ≥ 3.20, vcpkg, MSVC (Windows) — 詳細は [genmesh README](tools/genmesh/README.md)

### ビルド

```bash
# Rust ワークスペース全体をビルド
cargo build

# sdf-baker のみ
cargo build -p sdf-baker

# genmesh (C++)
cd tools/genmesh
cmake --preset default
cmake --build --preset default
```

### 実行

```bash
# 内蔵球 SDF でパイプライン全体を実行
cargo run -p sdf-baker -- \
  --out output/ \
  --genmesh-path tools/genmesh/build/RelWithDebInfo/genmesh.exe \
  --force

# 外部 WGSL シェーダを指定
cargo run -p sdf-baker -- \
  --shader my_sdf.wgsl \
  --out output/ \
  --aabb-size 128,128,128 \
  --genmesh-path tools/genmesh/build/RelWithDebInfo/genmesh.exe

# ブリック書き出しのみ (genmesh スキップ)
cargo run -p sdf-baker -- --out output/ --skip-genmesh --force
```

### テスト

```bash
# Rust テスト (sdf-baker: 61 tests)
cargo test -p sdf-baker

# genmesh C++ テスト
cd tools/genmesh
ctest --preset default
```

## ディレクトリ構成

```
print-organizer/
├── Cargo.toml              # Rust ワークスペースルート
├── src/                    # GUI アプリ (eframe + wgpu)
├── crates/
│   └── sdf-baker/          # SDF ベイクパイプライン CLI
│       ├── src/
│       ├── tests/          # 61 テスト (GPU 統合テスト含む)
│       └── shaders/        # compute テンプレート (WGSL/GLSL)
├── tools/
│   └── genmesh/            # C++ メッシュ生成 CLI (OpenVDB)
│       ├── src/
│       ├── include/
│       └── tests/          # 11 テストスイート
├── docs/
│   ├── plan.md             # 計画書
│   ├── schemas/            # JSON スキーマ (manifest, bricks-index, report)
│   └── spec/               # genmesh 仕様書
└── LICENSE                 # MIT
```

## データフロー

1. **SDF シェーダ** — ユーザーが `fn sdf(p) → f32` を WGSL または GLSL で定義
2. **GPU Bake** — wgpu compute shader で距離場をブリック単位 (64³) に評価
3. **スパース最適化** — 表面を含まないブリックをスキップ
4. **ブリック書き出し** — `manifest.json` + `bricks.bin` + `bricks.index.json`
5. **genmesh** — OpenVDB FloatGrid 構築 → `volumeToMesh` → バイナリ STL
6. **レポート** — `report.json` に統計・タイミング・エラーを記録

## ドキュメント

- [計画書](docs/plan.md) — プロジェクト全体の設計・スコープ
- [genmesh 仕様](docs/spec/genmesh_v1.md) — genmesh v1 仕様
- JSON スキーマ:
  - [manifest.v1.schema.json](docs/schemas/manifest.v1.schema.json)
  - [bricks-index.v1.schema.json](docs/schemas/bricks-index.v1.schema.json)
  - [report.v1.schema.json](docs/schemas/report.v1.schema.json)

## ライセンス

[MIT](LICENSE)
