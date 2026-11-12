Commit: 57a7f31 Add Japanese chapters 4 to 6, CI check, ADR 0069 and document links (102f)
Files: docs/security-pack/ja/04-logging-and-retention.md, docs/security-pack/ja/05-personal-data.md, docs/security-pack/ja/06-security-questionnaire.md, docs/security-pack/README.md, docs/adr/0069-product-security-pack.md, docs/release-checklist.md, docs/status-and-roadmap.md, docs/requirements.md, README.md, .github/workflows/ci.yml

Done when:
1. `sh scripts/check-security-pack.sh --complete` -> `security pack ok (12 files)`, exit 0.
2. `sh scripts/check-security-pack-test.sh` -> `all cases passed`.
3. Q rows: en 71, ja 71 (`grep -c '^| Q-'`).
4. Status-word grep on ja/04-06 -> nothing.
5. Source-bullet diff: NOT empty for 04, 05 and 06 (cannot be met as written), same cause as 102e: the `- \`` pattern also matches body bullets that start with a backticked word. 04: `RUST_LOG` bullet (4.2). 05: `AEGIS_EVENT_RETENTION_DAYS` bullet (5.3). 06: the six status-word bullets of 6.1. Those are translated on purpose. The bullets in the Sources sections are identical (the script's R4/R5 pass).
6. `grep -c 'run: scripts/check-security-pack' ci.yml` -> 2; `yaml.safe_load` ok (yaml module present).
7. `grep -n 'security-pack'` hits README.md, release-checklist.md, requirements.md; `^| 102 ` one line (81); ADR file exists (68 lines).
8. `cargo fmt --all` ok; `scripts/ci.sh` -> fmt, clippy, test ok, "all steps passed in 8 crate(s)".

Notes:
New terms (English -> Japanese):
- Sources -> 出典 ; Integrity -> 完全性 ; Retention -> 保存期間 (the rule/policy) ; trimmed -> 削減 ; trim -> 削減
- process log -> プロセスログ ; standard output / error -> 標準出力 / 標準エラー出力 ; operation history -> 操作履歴
- actor -> 実行者 ; subject (token) -> サブジェクト ; target -> 対象 ; detail -> 詳細 ; kind -> 種類
- Status (column) -> 状況 ; Reference -> 参照先 ; Answer -> 回答 ; Question -> 質問
- minimisation -> データの最小化 ; storage limitation -> 保存の制限 ; by design and by default -> データ保護バイデザインとバイデフォルト ; records of processing -> 処理の記録 ; breach -> 侵害 ; transfers -> 移転
- Rights: Access アクセス, Correction 訂正, Deletion 削除, Restriction 制限, Portability ポータビリティ, Objection 異議
- software bill of materials -> ソフトウェア部品表 ; penetration test -> ペネトレーションテスト ; fuzzing -> ファジング ; property test -> プロパティテスト ; static analysis -> 静的解析 ; memory-safe -> メモリー安全な ; Cyber Resilience Act -> サイバーレジリエンス法 ; high availability -> 高可用性 ; grace period -> 猶予期間 ; subcontractors -> 下請け業者 ; contract papers -> 契約書類 ; proof of concept -> 概念実証 ; multi-tenant -> マルチテナント ; single sign-on -> シングルサインオン
- "Customer" as a role word in running text is `お客様` in backticks where English has backticks; plain "the customer" -> お客様 (the legal-term sentence in 5.1 uses 管理者 and 個人情報取扱事業者 as the prompt says).
- Chapter references: "chapter 3" -> 3 章 ; section numbers unchanged.
- The language says "unsafe" and "lint", "crate", "issue", "Miri", "cargo-deny", "Rust" kept in ASCII.

English that looked wrong or unclear (translated as written):
- 4.5 says "Both run at server start and then every hour"; fine, but 4.3 repeats the failed-login-name text three times (4.1, 4.3, PD-04); kept.
- 4.4 "The release checklist requires the check to pass on a database used by the previous release" is unclear (which database); kept.
- 5.2 PD-03 says sessions are "At most 10 per user" and 3.4 says the oldest is closed; consistent.
- 5.2 PD-07: "Deleting an account does not remove its membership rows" while Q-17 says the same; fine.
- 5.5 row "15 to 17" and "33 and 34" are article ranges as given; kept. "28 (processor)" is listed last, after 44 (order not numeric); kept.
- Q-33 "Miri tests, as the rule states" refers to the engineering standard without naming a rule number.
- Q-46 "within 24 hours, 72 hours and 14 days after a fix" lacks what each deadline is for; kept.
- Q-60: "history chain check" vs the section's "chain check"; kept.
- 4.2 LG-02 "In standalone mode, files in the project folder" while 5.2 PD-13 says the actor is fixed `standalone`; consistent.
- Q-12 says "slows an address after 20 failures"; 3.3 says the same.
- Q-62 "After that reading and listing still work" has the "After that" referring to the grace period end; kept.
- The 5.5 wording "Customer decides on and makes any notice" (33 and 34) is awkward; translated 通知をするかどうかの判断と通知は、お客様が行います.

For the reviewer:
- ADR 0069 date set to 2026-11-12 (today). Its item 6 (stale database TLS text corrected in docs/deployment.md) is taken from the prompt; I did not check the earlier slice that made the change (docs/deployment.md is outside this slice).
- The 5.3 sentence about the product not choosing the subject form is translated; the five legal-duty terms follow the prompt (利用目的の特定, 安全管理措置, 従業者の監督, 委託先の監督, 第三者提供, 外国にある第三者への提供, 開示・訂正・利用停止等の請求, 漏えい等の報告). The 5.6 table keeps the English row for "Supervision of employees and of entrusted parties" as one row.
- In 5.1, `個人情報保護法` is the short name after the first mention in the first bullet; the first mention of the full APPI name is in 5.1 only (the heading of the chapter, line 1, carries both short names as the English title does).

NEXT: reviewer
