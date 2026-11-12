Commit: b138e57 Add Japanese security pack chapters 1 to 3 (102e)
Files: docs/security-pack/ja/01-architecture-and-data-flow.md, docs/security-pack/ja/02-cryptography.md, docs/security-pack/ja/03-authentication-and-access.md

Done when:
1. `sh scripts/check-security-pack.sh` -> `security pack ok (9 files)`, exit 0.
2. Table row counts (`grep -c '^|'`) en vs ja: 01 = 35/35, 02 = 18/18, 03 = 14/14.
3. Source-bullet diff: empty for 01 and 02. NOT empty for 03 (cannot be met as written): the English file has two other bullets that start with "- `" (`- \`AEGIS_DEV_AUTH\` and ...` and `- \`Customer\` runs the identity provider ...`). Their Japanese lines are translated (the second starts with `お客様`, as step 4 requires), so they differ. The 12 source bullets in 3.9 are identical. Suggest the check use only the lines after the Sources heading.
4. `grep -n 'Planned\|Customer' docs/security-pack/ja/0[123]-*.md` -> nothing.
5. Removed `## 2.4` from a temp copy of ja/02: exit 1, output:
   FAIL ja/02-cryptography.md: heading '2.5' should be '2.4'
   FAIL ja/02-cryptography.md: heading '2.6' should be '2.5'
   FAIL ja/02-cryptography.md: heading '2.7' should be '2.6'
   FAIL ja/02-cryptography.md: heading numbers differ from en
   Copy deleted.
Also: `cargo fmt --all` ok; `scripts/ci.sh crates/aegis-license` all steps passed.

Notes:
Term list (English -> Japanese):
- Planned -> 計画中 ; Customer -> お客様 (written `お客様` in backticks where English has `Customer`)
- company login -> 会社ログイン (app word) ; identity provider -> ID プロバイダー (app: IDプロバイダー; I put a space between Latin and Japanese)
- standalone mode -> スタンドアロンモード ; server mode -> サーバーモード
- project folder -> プロジェクトフォルダー ; figure -> 図 ; baseline -> ベースライン ; event log -> イベントログ
- administrator -> 管理者 ; server administration -> サーバー管理 ; Manage (permission) -> 管理 ; Read/Write -> 読み取り/書き込み ; yes/no -> はい/いいえ
- "keep me logged in" -> 「ログイン状態を保持」 (from app string)
- password login -> パスワードログイン ; session -> セッション ; session token -> セッショントークン ; user name -> ユーザー名
- reviewer role etc. kept as role keys; member/membership -> メンバー/メンバーシップ
- data at rest -> 保存データ ; MFA/multi-factor -> 多要素認証 ; hash chain -> ハッシュチェーン ; trust boundary -> 信頼境界 ; key management -> 鍵管理
- keychain -> キーチェーン ; trust store -> トラストストア ; operating system -> オペレーティングシステム ; loopback -> ループバック
- throttling -> スロットリング ; offline bundle -> オフラインバンドル ; update manifest -> 更新マニフェスト ; digest -> ダイジェスト ; salt -> ソルト ; verifier -> ベリファイアー
- editing lock -> 編集ロック ; deployment mode -> デプロイモード ; data flow -> データフロー
- "the pack" -> 本資料 (one use, 2.1)

English that looked odd (translated as written):
- 3.5 role table: `admin` has Server administration "no", yet text says a server administrator is separate; fine, but `reviewer` and `methodology_owner` rows are identical to `viewer`.
- 3.4 "when the user is disabled" etc. fine. 1.6 line 89 of en is a long unwrapped line (formatting only).
- 2.2 CR-11: "signed with Ed25519 by the update key" translated literally.
- 1.8 / 3.8 repeat the no-MFA/no-reset statement (also 3.1, 3.3); kept.
Section 1.4 DF-05 "Update address" translated 更新アドレス.

NEXT: reviewer
