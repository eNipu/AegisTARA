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

# Party roles (job 145).
party-us = 自社
party-customer = 顧客
party-supplier = サプライヤー
party-other = その他
