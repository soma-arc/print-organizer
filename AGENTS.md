# AGENTS.md

## 行動規範

- 「調査せよ」「調べろ」「確認して」等の指示には、結果の報告のみ行い、コード変更・コミットは行わない。実装に進むには、調査結果を報告した上で実施可否を確認すること。
- 「実装せよ」と明示された場合のみコード変更を行う。

## 計画

- 計画文書は `docs/plan.md` にある。タスク着手前に必ず参照すること。

## コーディング規約

- 言語: Rust (edition 2024)
- フォーマット: `cargo fmt` に従う
- コミットメッセージ: Conventional Commits 形式 (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`)

## ビルド規約

- genmesh (C++) のビルドは `cmake --build --preset default` または `cmake --build . --config RelWithDebInfo` を使用すること
- `--config Release` は使用しない（sdf-baker は `RelWithDebInfo` を参照する）
