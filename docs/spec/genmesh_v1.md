# genmesh CLI 仕様書 (v1)

この文書は、コーディングエージェントに単独で渡せるように、I/Oと検証条件を仕様として固定する。

**関連スキーマ**:
- [manifest.v1.schema.json](../schemas/manifest.v1.schema.json)
- [bricks-index.v1.schema.json](../schemas/bricks-index.v1.schema.json)
- [report.v1.schema.json](../schemas/report.v1.schema.json)

---

## 1. 目的と責務

- Rust側の生成物（距離場＋manifest）を入力に、OpenVDBを用いて以下を実行する。
  - VDB構築（Transform/translation/ナローバンド整合）
  - メッシュ化（iso面抽出）
  - ポリゴン削減（必須：一次は `volumeToMesh` の `adaptivity` を使用）
  - 出力（STL必須、VDBは推奨）

## 2. コマンド形（最小）

- `genmesh --manifest <path> --in <path> --out <dir>`

### 2.1 入力探索（決定）

- **[D] `--manifest` と `--in` は省略不可**（自動探索しない）。
  - 理由: 探索が再現性とデバッグを損なうため。

### 2.2 推奨フラグ（最小セット）

- `--write-stl`（既定: true）
- `--write-vdb`（既定: false）
- `--iso <float>`（既定: manifest.iso。なければ 0.0）
- `--adaptivity <float>`（既定: manifest.adaptivity。なければ 0.0）
- `--log-level <error|warn|info|debug>`（既定: info）

### 2.3 デバッグ用フラグ（任意）

- `--debug-generate sphere|box`（CLI内部で距離場生成）
- `--in-dense-raw <path>`（dense入力：I/O層切り分け）

## 3. 入出力

- **入力（必須）**
  - `manifest`（project.json）
  - 距離場データ（ブリック推奨）
- **出力（必須）**
  - `mesh.stl`（mm単位、バイナリ）
  - `report.json`（統計・警告・実行時間等）
- **出力（推奨）**
  - `volume.vdb`（アーカイブ・追試用）
  - `mesh.obj` / `mesh.glb`（任意）

### 3.1 出力パス・命名規則（決定）

- **[D] `--out <dir>` は出力ディレクトリ**として扱う（ファイルパスではない）。
- **[D] v1のファイル名は固定**（カスタマイズ不可）。
  - `mesh.stl`
  - `report.json`
  - `volume.vdb`（`--write-vdb` のときのみ）
- **[D] manifestのプロジェクト名等はファイル名に混ぜない**（差分安定・再現性優先）。識別子は `report.json` / `project.json` 側に記録する。

### 3.2 ディレクトリ作成（決定）

- **[D] `--out` が存在しない場合は親を含めて作成する**（mkdir -p 相当）。
- 作成できない場合は終了コード 3（I/O）で失敗。

### 3.3 既存ファイル上書き（決定）

- **[D] 既存ファイルがある場合はエラー**（上書きしない）。
  - 例: `<dir>/mesh.stl` が存在 → 終了コード 3（I/O）
- **[D] `--force` でのみ上書き許可**（v1で追加）。
- **[D] 安全のため、書き込みはテンポラリ→renameのアトミック更新を推奨**（可能な範囲で）。

### 3.4 失敗時の出力の一貫性（推奨）

- 途中で失敗しても `report.json` は可能な限り残す（8.2参照）。
- `--force` 指定時でも、失敗時に既存成果物を壊さないように、テンポラリ書き込みを優先する。

## 4. Manifest 要件（v1・厳密）

**スキーマ**: [manifest.v1.schema.json](../schemas/manifest.v1.schema.json)

### 4.1 ファイル形式

- **v1ではJSON固定**（project.json）。
- 文字コード: UTF-8。
- 1つのmanifestが 1プロジェクト（= 1距離場入力 + 1出力群）を表す。
- YAML対応はv2以降の検討（必要性が出てから）。

### 4.2 必須フィールド

- `version: 1`（int）
- `coordinate_system`: object（必須。v1では値を固定し、変換は行わない）
  - `handedness`: "right"（string。v1では "right" 固定）
  - `up_axis`: "Y"（string。v1では "Y" 固定）
  - `front_axis`: "+Z"（string。v1では "+Z" 固定）
- `units`: "mm"（string。v1ではmm固定）
- `aabb_min: [x,y,z]`（float[3]、mm）
- `aabb_size: [sx,sy,sz]`（float[3]、mm、全成分 > 0）
- `voxel_size: <float>`（mm、>0）
- `dims: [nx,ny,nz]`（int[3]、全成分 >0）
- `sample_at: "voxel_center"`（string。v1ではcenter固定）
- `axis_order: "x-fastest"`（string。v1では x-fastest 固定）
- `distance_sign: "negative_inside_positive_outside"`（string。**v1ではこの値のみ許可**）
- `iso: <float>`（既定 0.0 推奨）
- `adaptivity: <float>`（0.0〜1.0）
- `narrow_band: { half_width_voxels: <int> }`（int、>=1）
- `brick: { size: <int> }`（int。既定 64。許可: 32/64/128）
- `dtype: "f16" | "f32"`（距離値の格納型）
- `background_value_mm: <float>`（band外の背景距離。既定 +1000.0 を推奨）
- `hashes: { manifest_sha256?: <hex>, bricks_bin_sha256?: <hex>, bricks_index_sha256?: <hex> }`（推奨だがv1ではフィールド自体は必須。値は任意）

### 4.3 整合性ルール（CLIが検証し、違反はエラー）

- **権威は `dims` と `voxel_size`** とする（CLIは `aabb_size` を検証に使うが、暗黙に補正しない）。
- `aabb_size[i]` は `dims[i] * voxel_size` と一致していること（許容誤差 `eps_mm = 1e-6`）。
- `coordinate_system.handedness == "right"` を要求（不一致はエラー。座標変換は行わない）。
- `coordinate_system.up_axis == "Y"` を要求（不一致はエラー。座標変換は行わない）。
- `coordinate_system.front_axis == "+Z"` を要求（不一致はエラー。座標変換は行わない）。
- `abs(aabb_size[i] - dims[i]*voxel_size) <= eps_mm` を要求。
- `adaptivity` は [0,1] にクランプせず、範囲外はエラー。
- `narrow_band.half_width_voxels` は voxels 単位で解釈し、world量（mm）としては解釈しない。
- `brick.size` は {32,64,128} のいずれか。
- `background_value_mm` は `> 0` かつ `>= narrow_band.half_width_voxels * voxel_size` を要求（背景がbandより小さいと符号付き距離の意味が崩れるため）。

### 4.4 追跡用フィールド（推奨・未指定でも動作）

- `shader: { path, hash }`
- `git: { commit }`
- `generator: { name, version, build_id }`
- `hardware: { gpu, driver, os }`
- `timestamps: { created_at }`

## 5. 距離場データ要件（ブリック・厳密）

**スキーマ**: [bricks-index.v1.schema.json](../schemas/bricks-index.v1.schema.json)

### 5.1 前提（距離値の意味）

- これは **符号付き距離場（SDF）**。
- 値の単位は **mm**。
- `distance_sign` に従い、既定は「内部が負、外部が正」。
- `iso` 面（既定 0.0）がメッシュ化対象。

### 5.2 サンプリング規約

- `sample_at == voxel_center` 固定。
- voxel index `(i,j,k)` のワールド座標は次で定義する。
  - `p = aabb_min + voxel_size * (vec3(i,j,k) + 0.5)`
- これ以外（角サンプル等）は v1 では受け付けない。

### 5.3 ブリック分割

- `brick.size = B`（例: 64）。ブリックは `B×B×B`。
- ブリック座標 `(bx,by,bz)` は、voxel座標 `(i,j,k)` に対し
  - `bx = floor(i / B)` 等。
- ブリック内ローカル座標 `(lx,ly,lz)` は
  - `lx = i % B` 等。

### 5.4 物理ファイルフォーマット（v1）

入力ディレクトリ `--in <path>` は以下を含む。
- `bricks.bin`（必須）
- `bricks.index.json`（必須。ブリックメタ）

#### bricks.index.json（必須）

v1では以下を含む（最低限）。
- `version: 1`
- `brick_size: B`
- `dtype: "f16"|"f32"`
- `axis_order: "x-fastest"`
- `dims: [nx,ny,nz]`（manifestと一致）
- `bricks: [{ bx,by,bz, offset_bytes, payload_bytes, encoding, crc32? }]`

`encoding` は v1 では `raw` のみ必須対応（将来 zstd 等を追加可能）。
`crc32`（任意）はブリックpayload（raw bytes）に対するCRC32。

#### bricks.bin（必須）

- エンディアン: little-endian。
- 各ブリックのpayloadは **密**（B^3個の値）。
- 配列の並び（axis_order = x-fastest）:
  - index = `lx + B*(ly + B*lz)`
- dtype:
  - f16: IEEE 754 binary16（half）
  - f32: IEEE 754 binary32
- ブリック座標の有効範囲（CLIが検証）:
  - `bx in [0, ceil(nx/B)-1]`、同様に `by,bz`
  - 範囲外のブリックはエラー（入力破損 or 座標系不一致の疑い）。

### 5.5 スパース（省略）規約

- v1では、**ブリック単位**の省略のみ許可。
- `bricks.index.json` に存在しないブリックは「全セルが band 外」とみなし、距離値は `background_value_mm` として扱う。
- `background_value_mm` は必ず正で、外部（outside）として扱う。
- `bandWorld = narrow_band.half_width_voxels * voxel_size`。

### 5.6 入力検証（CLIが検証し、違反はエラー）

- bricks.index.json の `version==1`
- bricks.index.json の `brick_size/dtype/axis_order/dims` が manifest と一致
- 各ブリックの `payload_bytes == B^3 * sizeof(dtype)`（rawの場合）
- `offset_bytes + payload_bytes` が `bricks.bin` の範囲内
- 同一 `(bx,by,bz)` の重複定義がない
- （任意）`crc32` がある場合は一致を検証（不一致はエラー）

## 6. VDB構築ルール（CLI内部）

- 座標系規約（v1・固定）
  - `coordinate_system.handedness == "right"` のみ許可
  - `coordinate_system.up_axis == "Y"` のみ許可
  - `coordinate_system.front_axis == "+Z"` のみ許可
  - 不一致はエラー（座標変換は行わない）
- `openvdb::initialize()` を必ず呼ぶ。
- `voxel_size` を `Transform::createLinearTransform(voxel_size)` に反映。
- `aabb_min` は Transform translation（または等価手段）で反映。
- **原点への平行移動は行わない**（`aabb_min` をそのまま使用）。
- Gridは level set を想定（grid classの明示を推奨）。
- ナローバンド: 入力がブリック省略を含むため、未入力領域は背景値として扱う。
- 符号規約:
  - v1では `distance_sign == "negative_inside_positive_outside"` のみ許可。
  - 不一致はエラー（自動反転はしない）。

## 7. メッシュ化ルール

- `volumeToMesh(grid, points, triangles, quads, iso, adaptivity)` を使用。
- `iso` 既定 0.0。
- `adaptivity` 既定 0.0。
- 出力は STL（バイナリ）を必須。

**座標系・座標変換（v1・決定）**
- STL出力時、manifestの座標系をそのまま出力する。
- 座標変換は行わない（Y-up→Z-up等の変換は行わない）。
- `aabb_min` のオフセットは維持（原点への平行移動は行わない）。
- `volumeToMesh` が生成した頂点座標を、OpenVDBのTransformでワールド座標に変換し、そのままSTLに書き込む。

### 7.1 STL書き出し（決定）

- **[D] バイナリSTLヘッダー(80B)**: 固定文字列（例: `"Generated by genmesh"`）を先頭に入れ、残りは空白/0埋め。
  - バージョンやタイムスタンプは **ヘッダーに入れない**（再現性・差分安定を優先）。
  - 代わりに `report.json` に `generator/version/timestamps` を記録する。
- **[D] winding/法線**:
  - v1では `volumeToMesh` の出力順をそのままSTLへ書く（windingの自動反転はしない）。
  - 出力時に各三角形の法線は **頂点から再計算**（`normalize(cross(v1-v0, v2-v0))`）。
  - 面積が極小で法線が不定（|cross|が閾値以下）の場合は `(0,0,0)` を書く。
- **[D] quadsの扱い**:
  - `volumeToMesh` の `quads` は2三角形に分割してSTLへ書く。
  - 分割は **固定パターン**: `(0,1,2)` と `(0,2,3)`。
  - 追加の最適化（短い対角線選択など）は v2以降（必要性が出てから）。

### 7.2 退行検知（推奨）

- `--debug-generate sphere` のfixtureで、三角形数・AABB・（任意で）体積符号の整合をreportに記録し、winding反転が疑われる場合は警告を出す（自動修正はしない）。

## 8. report.json（出力・必須）

**スキーマ**: [report.v1.schema.json](../schemas/report.v1.schema.json)

- 目的: 退行検知・デバッグ・アーカイブ。
- **原則**: 成功・失敗に関わらず、`--out <dir>` が作成できる限り `report.json` を出力する。

### 8.1 タイムスタンプ（決定）

- **[D] ISO 8601 / UTC固定**（`Z`）。例: `2026-02-14T01:30:45Z`
- ローカル時刻（+09:00等）は **書かない**（混乱回避）。必要なら表示側で変換する。

### 8.2 report.json 詳細スキーマ（v1・決定）

ルート必須フィールド:
- `schema_version: 1`
- `status: "success"|"failure"`
- `stage: "validate"|"read"|"vdb_build"|"meshing"|"write"`（失敗時は停止時点）
- `started_at_utc: <iso8601>`
- `ended_at_utc: <iso8601>`（可能なら）
- `inputs: { manifest_path, in_dir, bricks_path?, dtype, brick_size, dims, voxel_size }`
- `timing_ms: { total, validate?, read?, vdb_build?, meshing?, write? }`
- `stats: { aabb_min, aabb_max, brick_count, triangle_count, quad_count, vertex_count, degenerate_count, mesh_aabb_min?, mesh_aabb_max?, active_voxel_count?, memory_usage_mb? }`
- `warnings: [ { code, message, kind?, context?, hint? } ]`
- `errors: [ { code, kind, message, context?, hint?, caused_by? } ]`

**timing_ms 定義（決定）**
- `validate`: manifest/index/binの整合チェック（パース＋検証）。
- `read`: bricks.index.json と bricks.bin の読み取り（必要ならCRC検証も含む）。
- `vdb_build`: Transform設定・grid生成・ボクセル挿入・背景設定。
- `meshing`: `volumeToMesh` 実行（＋quad→tri分割を含む）。
- `write`: STL/VDB/report 等の書き込み（テンポラリ→rename含む）。
- `total`: プロセスとして計測した全体。

**warnings/errors の構造化（決定）**
- `code` は `GENMESH_[E|W]<4桁>`。
- `message` は人間向け短文。
- `kind` は分類（例: `"validation"|"io"|"env"|"vdb"|"meshing"|"unexpected"`）。
- `context` は機械可読な追加情報（例: `{"path":"...","brick":[5,3,2]}`）。
- `hint` はユーザーが次に取るべき行動（任意）。

**stats（追加の扱い）**
- `quad_count` と `degenerate_count` は常に出す。
- `mesh_aabb_min/max` はメッシュが生成できた場合のみ出す。
- `active_voxel_count` / `memory_usage_mb` は推奨（計測できる場合のみ）。

### 8.3 失敗時のreport方針（決定）

- **[D] 可能な限り出す**: 例外は「`--out` が作れない/書けない」「プロセス開始直後の致命的例外」のみ。
- **[D] partial情報を含める**: どこまで進んだかを `stage` と `progress` で記録する。
  - `stage: "validate"|"read"|"vdb_build"|"meshing"|"write"`
  - `progress: { stage, percent?, detail? }`
- **[D] errorsは構造化**:
  - `errors: [{ code, kind, message, context?, hint?, caused_by? }]`

## 9. エラー取り扱い（終了コード・stderr・コード体系）

### 9.1 終了コード（決定）

- 0: 成功
- 2: 入力検証失敗（manifest/bricksの不整合、必須キー欠落、範囲外など）
- 3: ファイルI/Oエラー（open/read/write/create_dir、パーミッション、存在しない等）
- 4: 依存/環境エラー（OpenVDB初期化失敗、必要DLL不足、CPU命令/ランタイム不備など）
- 5: 処理エラー（VDB構築失敗、meshing失敗、メモリ不足、数値不正でアルゴリズムが進めない等）
- 1: その他の一般エラー（未分類・バグ）

### 9.2 stderrの形式（決定）

- **[D] stderrはプレーンテキスト**（人間が読むのを優先）。
- ただし各行は機械が拾えるように、先頭に最低限のフィールドを付ける。
  - 形式: `LEVEL CODE: message | key=value key=value ...`
  - 例: `ERROR GENMESH_E2001: manifest missing field | field=dims path=project.json`
- `--log-format json`（将来拡張・v2以降）を想定するが、v1では不要。

### 9.3 エラーコード体系（決定）

- 形式: `GENMESH_[E|W]<4桁>`（E=error, W=warning）
- 範囲と意味（例）:
  - E1xxx: 入力/検証（manifest/index/bin）
  - E2xxx: I/O（読み書き、パス、権限）
  - E3xxx: 環境/依存（OpenVDB初期化、DLL、バージョン）
  - E4xxx: VDB構築（Transform、grid生成、背景値）
  - E5xxx: meshing（volumeToMesh、出力メッシュ整合）
  - E9xxx: 想定外（例外/バグ）
- report.jsonの `errors[].code` はこのコードを必須とする。

### 9.4 代表コード（例）

- `GENMESH_E1001`: manifest必須フィールド欠落
- `GENMESH_E1002`: manifest整合性違反（aabb_size != dims*voxel_size）
- `GENMESH_E1101`: bricks.index.json 不整合（dims/dtype/brick_size不一致）
- `GENMESH_E2001`: bricks.bin read失敗
- `GENMESH_E2101`: report.json write失敗
- `GENMESH_E3001`: openvdb::initialize 失敗
- `GENMESH_E5001`: volumeToMesh 失敗

## 10. テスト要件

- `fixtures/` に最小入力（manifest+brick）を配置し `ctest` で回る。
- ゴールデンはバイナリ一致ではなく、統計（頂点/三角形/AABB等）で比較。
- `--debug-generate` によりRust無しで再現できる（sphere/box を最低限）。
