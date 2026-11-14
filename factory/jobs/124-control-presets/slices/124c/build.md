Commit: cd39212 Add example setting presets to the ISO, CRA and STRIDE packs (124c)
Files: crates/aegis-model/packs/iso21434-r155.json, crates/aegis-model/packs/eu-cra.json, crates/aegis-model/packs/stride-services-cloud.json, crates/aegis-model/tests/builtin_pack.rs, crates/aegis-model/tests/stride_pack.rs

Done when:
1. `cargo test --locked -p aegis-model --test builtin_pack` - passed (equals expected() and writer output).
2. `cargo test --locked -p aegis-model --test builtin_pack --test stride_pack` - 15 + 4 passed.
3. `scripts/ci.sh crates/aegis-model crates/aegis-client` - all steps passed.
4. Owner check: tables below.

Notes:
- Pack JSON was generated from the prompt tables by a scratch script (not committed); the first write matched the writer byte for byte, no hand-tuning.
- Pack versions unchanged. No file under src/ touched. All attack type and level keys were accepted by the loader.
- `CONTROL_KINDS` in builtin_pack.rs carries `#[allow(clippy::type_complexity)]` for the 5-tuple table type (clippy passed).
- CRA presets test was added inside `eu_cra_pack_is_canonical_and_evaluates_plain_likelihood`.

## Presets for the owner to review

### ISO pack (iso21434-r155.json)
| key | label | interface_kinds | rules | ja label |
|---|---|---|---|---|
| secure-boot | Secure boot | (component) | firmware-modification: low; bootloader-exploit: low | セキュアブート |
| secure-flash | Secure flash (signed updates) | (component) | firmware-modification: low | セキュアフラッシュ（署名付き更新） |
| flash-read-protection | Flash read-out protection | (component) | flash-read-out: low | フラッシュ読み出し保護 |
| hsm-key-storage | Keys stored in an HSM | (component) | flash-read-out: low; side-channel: low | HSMでの鍵保管 |
| housing-sealed | Housing sealed (tamper evident) | (component) | physical-tampering: low | 筐体の封印（開封検知） |
| debug-port-locked | Debug port locked | debug | debug-access: low; flash-read-out: low | デバッグポートのロック |
| debug-port-fused | Debug port fused off | debug | debug-access: very-low; flash-read-out: very-low | デバッグポートの無効化（ヒューズ） |
| secure-diagnostics | Secure diagnostics (seed and key) | can, ethernet, usb | diagnostic-access: medium | セキュア診断（シード＆キー） |
| diagnostic-auth-hsm | Authenticated diagnostics with HSM | can, ethernet, usb | diagnostic-access: low | HSMによる診断認証 |
| secoc | Message authentication (SecOC) | can, ethernet | local-bus-injection: low | メッセージ認証（SecOC） |
| network-firewall | Firewall and network segmentation | ethernet, cellular | remote-network: low | ファイアウォールとネットワーク分離 |
| wireless-pairing-auth | Authenticated wireless pairing | wireless | wireless-proximity: low | 認証付き無線ペアリング |

### CRA pack (eu-cra.json), all for components
| key | label | rules | ja label |
|---|---|---|---|
| secure-boot | Secure boot | firmware-modification: low; bootloader-exploit: low | セキュアブート |
| secure-flash | Secure flash (signed updates) | firmware-modification: low | セキュアフラッシュ（署名付き更新） |
| flash-read-protection | Flash read-out protection | flash-read-out: low | フラッシュ読み出し保護 |
| hsm-key-storage | Keys stored in an HSM | flash-read-out: low; side-channel: low | HSMでの鍵保管 |
| housing-sealed | Housing sealed (tamper evident) | physical-tampering: low | 筐体の封印（開封検知） |
| debug-port-locked | Debug port locked | debug-access: low; flash-read-out: low | デバッグポートのロック |
| network-firewall | Firewall and network segmentation | remote-network: low | ファイアウォールとネットワーク分離 |
| wireless-pairing-auth | Authenticated wireless pairing | wireless-proximity: low | 認証付き無線ペアリング |

### STRIDE pack (stride-services-cloud.json), all for components
| key | label | rules | ja label |
|---|---|---|---|
| network-segmentation | Network segmentation and firewall | network-exposure: low | ネットワーク分離とファイアウォール |
| mfa | Multi-factor authentication | credential-theft: low | 多要素認証 |
| secrets-vault | Secrets in a managed vault | credential-theft: low | マネージドボールトでのシークレット管理 |
| input-validation | Input validation and parameterized queries | injection: low | 入力検証とパラメータ化クエリ |
| waf | Web application firewall | injection: medium | Webアプリケーションファイアウォール |
| hardened-baseline | Hardened configuration baseline | misconfiguration: low | 堅牢化された構成ベースライン |
| signed-artifacts | Signed build artifacts | supply-chain: low | 署名付きビルド成果物 |
| datacenter-access | Physical access control at the data centre | physical-access: very-low | データセンターの物理アクセス制御 |

NEXT: reviewer
