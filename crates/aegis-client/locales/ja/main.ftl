# AegisTARA クライアントコアの文字列（ページ・エラー・レポート）。日本語。
# メッセージIDはロケール間で固定。欠けたIDは `[id]` と表示される。

report-title = { $project } - TARAレポート
report-subtitle = 脅威分析およびリスクアセスメント。手法: { $methodology }。サーバーの状態: 変更 { $seq }。エンジン { $engine }。
draft-warning = 下書き: このレポートには、まだサーバーに保存されていない変更が含まれています。
project-notes = プロジェクトのメモ
descriptions = 説明

section-executive-summary = 1. エグゼクティブサマリー
section-item-definition = アイテム定義
item-description = アイテムの説明
item-scope = 分析の範囲
section-figures = 図表
section-risks = 6. リスクレジスタと対応
section-problems = 9. 未解決の問題
section-comparison = ベースラインとの比較

risks-by-level = リスクレベル別
risk-level = リスクレベル
before-controls = 対策前
after-controls = 対策後
highest-risks = 最も高いリスク
summary-counts = このプロジェクトには { $assets } 件の{ $assets_label }上に { $threats } 件の{ $threats_label }があります。計画され検証された対策の後、最も高い2つのリスクレベルに残るのは { $above } 件の{ $threat_singular }です。{ $errors } 件のブロックする問題と { $warnings } 件の警告が未解決です。

threats = 脅威
yes = あり
not-rated = 未評価
none = なし。
none-lowercase = なし
field = 項目
value = 値
category = カテゴリ
catalog-references = カタログ参照
severity = 重大度
problem = 問題
blocking = ブロック
warning = 警告
goals = 目標
claims = 主張
generated-by = AegisTARA が生成しました。

threat-scenario = 脅威シナリオ
asset = 資産
deciding-damage = 決定する損害
impact = 影響
feasibility = 攻撃可能性
risk-before = 対策前リスク
risk-after = 対策後リスク
risk-before-controls = 対策前リスク
risk-after-controls = 対策後リスク
treatment = 対応
goal-or-claim = 目標または主張
control = 対策
attack-steps = 攻撃ステップ
attack-step = 攻撃ステップ
through = 経由
attack-type = 攻撃タイプ
rationale = 根拠
feasibility-rationale = 実現可能性の根拠
pair-rationale = 組み合わせの根拠
scales = スケール
scaling-reason = スケーラビリティの根拠
no = なし

tree-summary = { $nodes } ノード。ルートの攻撃可能性: { $feasibility }。{ $threats_plural }: { $threats }。
tree-caption = 攻撃ツリー
tree-overview-caption = ツリー全体の概要。詳細パネルが続きます。
tree-detail-caption = 詳細: { $row } / { $rows } 行目、{ $col } / { $cols } 列目
tree-omitted = あと { $count } 枚の詳細パネルは描画されていません。下のアウトラインに全ノードを記載しています。

sheet-overview = 概要
sheet-risk-register = リスクレジスタ
sheet-problems = 問題
sheet-figures = 図表

component = コンポーネント
parent = 親コンポーネント
kind = 種別
from-interface = 送信元インターフェース
to-interface = 送信先インターフェース
members = メンバー
figures = 図表
caption = キャプション
target = 対象
format = 形式
dimensions = サイズ
size-bytes = バイト数

report-language-en = 英語
report-language-ja = 日本語
report-language-bilingual = 英語と日本語
classification-none = （なし）
classification-internal = 社内限
classification-confidential = 機密
classification-secret = 極秘
approver = 承認者
approvers = 承認者
revision = 改訂
document-id = 文書ID
classification = 区分
date = 日付
description = 説明
methodology = 手法
report-systems = システム
report-all-systems = すべてのシステム
reduced-by = 低減策
reduced-by-none = 低減なし（決定ステップ: { $step }）
risks-lowered = 低減するリスク数
security-settings = セキュリティ設定
security-settings-note = コンポーネントの設定は、そのインターフェースにも適用されます。
applied-to = 適用先
setting = 設定
rules = ルール

# Table column headings (UI-14).
id = ID
properties = 特性
worst-risk = 最大リスク
controls = 対策
responsible = 責任者

# Relation labels (job 149).
relation-asset-component = コンポーネント
relation-threat-asset = 資産
relation-threat-damages = 損害シナリオ
relation-threat-tree = 攻撃ツリー
relation-threat-goal = 目標
relation-threat-claim = 主張
relation-goal-controls = 対策
relation-goal-responsible = 責任者
relation-claim-responsible = 責任者
relation-control-targets = 適用先
relation-step-via = 経由
relation-step-mitigations = 緩和策
relation-dataflow-from = フロー元
relation-dataflow-to = フロー先
relation-dataflow-interface = フローのインターフェース
relation-interface-component = 所属コンポーネント
relation-boundary-members = メンバーコンポーネント
condition = 条件
condition-present = 成立
relation-asset-conditions = 資産の条件
relation-component-conditions = コンポーネントの条件
relation-interface-conditions = インターフェースの条件
relation-threat-conditions = 脅威の条件
relation-tree-conditions = 攻撃ツリーの条件
relation-goal-conditions = 目標の条件
relation-claim-conditions = 主張の条件
relation-step-conditions = ステップの条件

# Party roles (job 145).
party-us = 自社
party-customer = 顧客
party-supplier = サプライヤー
party-other = その他

# Workbench commands (job 182)
cmd-palette = どこへでも移動
cmd-palette-tip = コマンドや要素をIDまたは名前で探します。
cmd-open-folder = フォルダーを開く…
cmd-open-folder-tip = プロジェクトフォルダーを開きます。
cmd-save = 保存
cmd-save-tip = 変更をプロジェクトフォルダーに保存します。
cmd-save-as = 名前を付けて保存…
cmd-save-as-tip = プロジェクトを新しいフォルダーに保存します。
cmd-undo = 元に戻す
cmd-undo-tip = 直前の変更を取り消します。
cmd-redo = やり直す
cmd-redo-tip = 取り消した変更をもう一度行います。
cmd-toggle-explorer = エクスプローラーの表示切替
cmd-toggle-explorer-tip = 左側のプロジェクトツリーを開閉します。
cmd-toggle-inspector = インスペクターの表示切替
cmd-toggle-inspector-tip = 右側の詳細パネルを開閉します。
cmd-toggle-bottom-dock = 下部パネルの表示切替
cmd-toggle-bottom-dock-tip = 画面下部のパネルを開閉します。
cmd-focus-explorer = エクスプローラーへ移動
cmd-focus-explorer-tip = キーボード操作をプロジェクトツリーに移します。
cmd-filter-explorer = エクスプローラーを絞り込む
cmd-filter-explorer-tip = 名前が一致する項目だけをツリーに表示します。
cmd-explorer-down = 次の項目
cmd-explorer-down-tip = 下の項目を選択します。
cmd-explorer-up = 前の項目
cmd-explorer-up-tip = 上の項目を選択します。
cmd-explorer-expand = 展開
cmd-explorer-expand-tip = この項目の中身を表示します。
cmd-explorer-collapse = 折りたたむ
cmd-explorer-collapse-tip = この項目の中身を隠します。
cmd-open-selected = 開く
cmd-open-selected-tip = 選択した項目をタブで開きます。
cmd-pick-up = 移動のために選ぶ
cmd-pick-up-tip = 選択した項目を移動対象として記憶します。
cmd-drop-here = ここへ移動
cmd-drop-here-tip = 選んだ項目を選択中の項目の下へ移動します。
cmd-close-tab = タブを閉じる
cmd-close-tab-tip = 表示中のタブを閉じます。
cmd-next-tab = 次のタブ
cmd-next-tab-tip = 右隣のタブへ移動します。
cmd-previous-tab = 前のタブ
cmd-previous-tab-tip = 左隣のタブへ移動します。
cmd-toggle-theme = ライト／ダーク切替
cmd-toggle-theme-tip = 明るい表示と暗い表示を切り替えます。
cmd-reset-layout = レイアウトをリセット
cmd-reset-layout-tip = すべてのパネルを最初の位置に戻します。
cmd-show-shortcuts = キーボードショートカット
cmd-show-shortcuts-tip = すべてのコマンドとそのキーを一覧表示します。
cmd-quit = 終了
cmd-quit-tip = AegisTARAを閉じます。

# Workbench explorer (job 182)
explorer-overview = 概要
explorer-project-definition = プロジェクト定義
explorer-parties = 責任当事者
explorer-shared-analysis = 共有分析
explorer-systems = システム
explorer-not-in-system = システム未割り当て
explorer-item-definition = アイテム定義
explorer-risk-treatment = リスク対応
explorer-goals-and-claims = 目標と主張
explorer-problems = 問題

# Workbench window (job 182)
workbench-overview = 概要
workbench-inspector-empty = 要素を選択すると詳細が表示されます。
workbench-bottom-empty = 問題、変更、タスクの出力は今後のバージョンでここに表示されます。
workbench-status-unsaved = サンプルプロジェクトはメモリ上にあります。変更は保存されません。
workbench-status-saved = 保存済み
workbench-status-changed = 未保存の変更があります
workbench-converted = 旧形式のプロジェクトから変換しました。旧ファイルはフォルダーに残っています。
workbench-leave-title = 「{ $name }」の変更を保存しますか？
workbench-leave-sample = このプロジェクトはメモリ上にのみあります。保存しないと変更は失われます。
workbench-leave-failed-title = 「{ $name }」を保存できませんでした
workbench-leave-failed = { $error } 保存しないと、前回の保存以降の変更は失われます。
workbench-leave-save-as = 名前を付けて保存…
workbench-leave-retry = 再試行
workbench-leave-discard = 保存しない
workbench-leave-cancel = キャンセル
workbench-explorer = エクスプローラー
workbench-inspector = インスペクター
workbench-bottom = 出力

# Workbench explorer (job 182)
workbench-picked = { $name } を移動中: システムを選択して { $keys } を押してください。
workbench-list-empty = まだ何もありません。

# Sign-in screen (job 208)
signin-choose-server = AegisTARA サーバーのアドレスを入力してください。
signin-connecting = { $url } に接続中…
signin-online = { $user } としてサインインしています。
signin-offline = サーバーに接続できません。{ $reason }
signin-login-company = 会社のアカウントでログインしてください。
signin-login-password = ユーザー名とパスワードでログインしてください。
signin-login-dev = ユーザー名を入力してください。
signin-error-credentials = ユーザー名またはパスワードが正しくありません。
signin-error-throttled = ログインの失敗が多すぎます。{ $seconds } 秒後にもう一度お試しください。
signin-error-password-off = このサーバーではパスワードログインがオフです。会社のログインを使用してください。
signin-error-password-turned-off = パスワードログインがオフになりました。会社のアカウントでログインしてください。
signin-error-unreachable = サーバーに接続できません: { $error }
signin-error-not-reached = { $url } に接続できませんでした。{ $error }
signin-error-too-old = このサーバーはこのアプリには古すぎます（ログイン方法を返せません）。サーバーを更新してください。
signin-error-login-unfinished = ログインが完了しませんでした: { $error }
license-unreadable = ライセンスファイル { $path } を読み込めません: { $error }
license-invalid = ライセンスファイルを使用できません: { $error }
license-not-yet-valid = このライセンスはまだ有効ではありません。コンピューターの日付を確認してください。
workbench-needs-license = スタート画面から、ライセンスファイルを指定してプロジェクトフォルダーを開いてください。
workbench-read-only-expired = 読み取り専用: { $customer } のライセンスの有効期限が切れています。
