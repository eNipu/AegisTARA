//! Curated public-source cybersecurity and TARA seed bank (Task AI-03).
//!
//! All content in this module is original wording synthesized from permissive
//! public ontologies (MITRE CAPEC attack patterns, MITRE EMB3D embedded device
//! properties, UN Regulation No. 155 Annex 5 threat catalog numbers, and EU CRA
//! embedded security concepts). No copyrighted ISO/SAE 21434 prose is included
//! (Decision D14).
//!
//! Includes both English (`en`) and Japanese (`ja`) examples (Decision D13) as
//! well as out-of-domain prompts so the model learns to emit
//! `<|out_of_domain|>` for anything outside TARA and cybersecurity.

use aegis_model::GateKind;
use aegis_model::library::ItemNode;
use serde::Deserialize;

use crate::dsl::ThreatProposal;

/// Embedded open-source cybersecurity catalog (`packs/open_cyber_en.json`)
/// compiled from MITRE EMB3D v2.0.1 and MITRE CAPEC STIX 2.1.
const OPEN_CYBER_EN_JSON: &str = include_str!("../packs/open_cyber_en.json");

/// A normalized open-source cybersecurity scenario loaded from
/// `packs/open_cyber_en.json` (MITRE EMB3D + MITRE CAPEC).
#[derive(Clone, Debug, Deserialize)]
pub struct OpenCatalogScenario {
    /// Stable scenario identifier (`emb3d-tid-101`, `capec-capec-125`, etc.).
    pub id: String,
    /// Open catalog source name (`MITRE-EMB3D-2.0.1`, `MITRE-CAPEC-STIX-2.1`).
    pub source: String,
    /// Methodology key (`iso21434-r155`).
    pub methodology_key: String,
    /// Language code (`en`).
    pub lang: String,
    /// Target asset name.
    pub asset_name: String,
    /// Asset interfaces (`can`, `obd`, `cellular`, `ethernet`, `debug-port`, `ble`, `ota`, `v2x`).
    pub interfaces: Vec<String>,
    /// Security properties (`integrity`, `authenticity`, `confidentiality`, `availability`).
    pub properties: Vec<String>,
    /// Threat scenario name.
    pub threat_name: String,
    /// STRIDE category key.
    pub stride: String,
    /// Catalog citations (`UN R155 Annex 5`, `TID-xxx`, `CAPEC-xxx`, `CWE-xxx`).
    pub catalog_ids: Vec<String>,
    /// Damage scenario name.
    pub damage_name: String,
    /// Impact rating (`category:level`).
    pub impact: String,
    /// Complete candidate attack tree.
    pub tree: ItemNode,
}

#[derive(Clone, Debug, Deserialize)]
struct OpenCatalogPack {
    scenarios: Vec<OpenCatalogScenario>,
}

/// Loads the built-in open-source English cybersecurity catalog (`packs/open_cyber_en.json`)
/// derived from MITRE EMB3D STIX 2.0.1 and MITRE CAPEC STIX 2.1.
///
/// # Panics
///
/// Panics only if the compile-time embedded `packs/open_cyber_en.json` is malformed.
#[must_use]
pub fn open_catalog_scenarios() -> Vec<OpenCatalogScenario> {
    let pack: OpenCatalogPack =
        serde_json::from_str(OPEN_CYBER_EN_JSON).expect("packs/open_cyber_en.json is valid JSON");
    pack.scenarios
}

/// A complete seed scenario linking an asset, interfaces, security properties,
/// threat proposals, and an attack tree in both English and Japanese.
#[derive(Clone, Debug)]
pub struct SeedScenario {
    /// Stable scenario identifier.
    pub id: &'static str,
    /// Methodology key (`iso21434-r155` or `med-device-demo`).
    pub methodology_key: &'static str,
    /// Language code (`en` or `ja`).
    pub lang: &'static str,
    /// Asset name.
    pub asset_name: &'static str,
    /// Asset interfaces (`can`, `obd`, `cellular`, `ethernet`, `debug-port`, `ble`, `ota`, `v2x`).
    pub interfaces: &'static [&'static str],
    /// Security properties (`integrity`, `availability`, `confidentiality`, `authenticity`, `authorization`).
    pub properties: &'static [&'static str],
    /// Primary threat scenario title.
    pub threat_name: &'static str,
    /// Primary STRIDE category key.
    pub stride: &'static str,
    /// Catalog citations (`UN R155 Annex 5`, `CAPEC`, `EMB3D`).
    pub catalog_ids: &'static [&'static str],
    /// Primary damage scenario name.
    pub damage_name: &'static str,
    /// Primary impact (`category:level`).
    pub impact: &'static str,
    /// Additional threat proposals for the same asset (used in `SuggestThreats`).
    pub threat_proposals: Vec<ThreatProposal>,
    /// Complete candidate attack tree for the primary threat scenario.
    pub tree: ItemNode,
}

/// Concrete slot bindings for parameterized templates (`{bus}`, `{ecu}`, `{function}`).
#[derive(Clone, Debug)]
pub struct SlotBindingSeed {
    /// Language code (`en` or `ja`).
    pub lang: &'static str,
    /// Bus or interface name (`CAN-FD chassis bus`, `Automotive Ethernet VLAN 10`, etc.).
    pub bus: &'static str,
    /// Target ECU name (`Brake Control ECU`, `EPS Steering ECU`, etc.).
    pub ecu: &'static str,
    /// Safety/operational function (`emergency deceleration`, `steering torque assist`, etc.).
    pub function: &'static str,
}

fn step(label: &str, feas: &str) -> ItemNode {
    ItemNode {
        label: label.to_owned(),
        gate: None,
        feasibility: Some(feas.to_owned()),
        children: Vec::new(),
    }
}

fn gate(kind: GateKind, label: &str, children: Vec<ItemNode>) -> ItemNode {
    ItemNode {
        label: label.to_owned(),
        gate: Some(kind),
        feasibility: None,
        children,
    }
}

/// Returns the built-in multi-domain bilingual seed scenarios.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn seed_scenarios() -> Vec<SeedScenario> {
    vec![
        SeedScenario {
            id: "can-brake-spoof-en",
            methodology_key: "iso21434-r155",
            lang: "en",
            asset_name: "Electronic Brake Control Module (EBCM)",
            interfaces: &["can", "obd", "debug-port"],
            properties: &["integrity", "authenticity", "availability"],
            threat_name: "Spoof deceleration request frames on chassis CAN bus",
            stride: "spoofing",
            catalog_ids: &["4.1", "CAPEC-151"],
            damage_name: "Unintended vehicle braking at highway speed",
            impact: "safety:severe",
            threat_proposals: vec![
                ThreatProposal {
                    name: "Spoof deceleration request frames on chassis CAN bus".into(),
                    stride: "spoofing".into(),
                    catalog_ids: vec!["4.1".into(), "CAPEC-151".into()],
                    damage_name: "Unintended vehicle braking at highway speed".into(),
                    impact: "safety:severe".into(),
                },
                ThreatProposal {
                    name: "Flood chassis CAN bus with high-priority arbitration frames".into(),
                    stride: "denial-of-service".into(),
                    catalog_ids: vec!["4.3", "CAPEC-125"]
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                    damage_name: "Loss of electronic brake assist commands".into(),
                    impact: "safety:severe".into(),
                },
                ThreatProposal {
                    name: "Reflash brake controller firmware via unauthenticated UDS routine"
                        .into(),
                    stride: "tampering".into(),
                    catalog_ids: vec!["3.1".into(), "CAPEC-441".into()],
                    damage_name: "Persistent compromise of braking control logic".into(),
                    impact: "safety:severe".into(),
                },
            ],
            tree: gate(
                GateKind::Or,
                "Trigger unintended braking on the EBCM",
                vec![
                    gate(
                        GateKind::SeqAnd,
                        "Inject forged braking frames from the OBD port",
                        vec![
                            step("Reach the OBD port", "low"),
                            step(
                                "Bypass gateway frame filtering between diagnostic and chassis CAN",
                                "medium",
                            ),
                            step("Transmit spoofed deceleration arbitration frames", "low"),
                        ],
                    ),
                    gate(
                        GateKind::SeqAnd,
                        "Compromise an adjacentECU on the chassis CAN bus",
                        vec![
                            step(
                                "Exploit a memory corruption flaw in the infotainment unit",
                                "medium",
                            ),
                            step("Pivot through the CAN transceiver interface", "medium"),
                            step(
                                "Forge SecOC message authentication codes using leaked key",
                                "high",
                            ),
                        ],
                    ),
                ],
            ),
        },
        SeedScenario {
            id: "can-brake-spoof-ja",
            methodology_key: "iso21434-r155",
            lang: "ja",
            asset_name: "電子ブレーキ制御モジュール (EBCM)",
            interfaces: &["can", "obd", "debug-port"],
            properties: &["integrity", "authenticity", "availability"],
            threat_name: "シャシーCANバス上で減速要求フレームを偽装する",
            stride: "spoofing",
            catalog_ids: &["4.1", "CAPEC-151"],
            damage_name: "高速走行中の意図しない車両減速",
            impact: "safety:severe",
            threat_proposals: vec![
                ThreatProposal {
                    name: "シャシーCANバス上で減速要求フレームを偽装する".into(),
                    stride: "spoofing".into(),
                    catalog_ids: vec!["4.1".into(), "CAPEC-151".into()],
                    damage_name: "高速走行中の意図しない車両減速".into(),
                    impact: "safety:severe".into(),
                },
                ThreatProposal {
                    name: "高優先度フレームでシャシーCANバスを溢れさせる".into(),
                    stride: "denial-of-service".into(),
                    catalog_ids: vec!["4.3".into(), "CAPEC-125".into()],
                    damage_name: "ブレーキ制御通信の喪失".into(),
                    impact: "safety:severe".into(),
                },
            ],
            tree: gate(
                GateKind::Or,
                "EBCMに偽装された制動コマンドを受理させる",
                vec![
                    gate(
                        GateKind::SeqAnd,
                        "OBDポートから偽装フレームを注入する",
                        vec![
                            step("OBDポートに接続する", "low"),
                            step(
                                "診断CANとシャシーCAN間のゲートウェイフィルタを迂回する",
                                "medium",
                            ),
                            step("偽装された減速CANフレームを送信する", "low"),
                        ],
                    ),
                    gate(
                        GateKind::SeqAnd,
                        "UDS診断サービス経由でEBCMファームウェアを書き換える",
                        vec![
                            step("OBDポートに接続する", "low"),
                            gate(
                                GateKind::Or,
                                "UDSセキュリティアクセスを通過する",
                                vec![
                                    step("診断用PINの推測", "low"),
                                    step("別個体からSeed-Key秘密鍵を抽出する", "high"),
                                ],
                            ),
                            step("改ざんされたファームウェアをフラッシュする", "medium"),
                        ],
                    ),
                ],
            ),
        },
        SeedScenario {
            id: "ota-firmware-tamper-en",
            methodology_key: "iso21434-r155",
            lang: "en",
            asset_name: "OTA Update Client & Telematics Gateway",
            interfaces: &["cellular", "ethernet", "ota"],
            properties: &["integrity", "authenticity", "confidentiality"],
            threat_name: "Deliver unsigned or rolled-back firmware package over OTA channel",
            stride: "tampering",
            catalog_ids: &["3.1", "3.2", "CAPEC-186"],
            damage_name: "Fleet-wide execution of unauthorized ECU firmware",
            impact: "operational:severe",
            threat_proposals: vec![
                ThreatProposal {
                    name: "Deliver unsigned or rolled-back firmware package over OTA channel"
                        .into(),
                    stride: "tampering".into(),
                    catalog_ids: vec!["3.1".into(), "CAPEC-186".into()],
                    damage_name: "Fleet-wide execution of unauthorized ECU firmware".into(),
                    impact: "operational:severe".into(),
                },
                ThreatProposal {
                    name: "Intercept OTA package metadata via cellular downgrade attack".into(),
                    stride: "information-disclosure".into(),
                    catalog_ids: vec!["7.2".into(), "CAPEC-94".into()],
                    damage_name: "Exposure of proprietary ECU firmware binaries".into(),
                    impact: "financial:major".into(),
                },
            ],
            tree: gate(
                GateKind::Or,
                "Install malicious firmware through the OTA update pipeline",
                vec![
                    gate(
                        GateKind::SeqAnd,
                        "Perform a version rollback attack over the cellular link",
                        vec![
                            step("Set up a rogue base station near the vehicle", "medium"),
                            step("Intercept the OTA manifest check", "medium"),
                            step(
                                "Replay an older signed firmware image with a known vulnerability",
                                "medium",
                            ),
                            step("Bypass anti-rollback counter check in bootloader", "high"),
                        ],
                    ),
                    gate(
                        GateKind::And,
                        "Sign malicious update using compromised backend signing key",
                        vec![
                            gate(
                                GateKind::Or,
                                "Compromise CI/CD release credentials",
                                vec![
                                    step("Phish an operator account", "medium"),
                                    step(
                                        "Extract API token from leaked build logs",
                                        "medium",
                                    ),
                                ],
                            ),
                            step(
                                "Submit unauthorized firmware build to signing service",
                                "high",
                            ),
                        ],
                    ),
                ],
            ),
        },
        SeedScenario {
            id: "ble-keyless-relay-en",
            methodology_key: "iso21434-r155",
            lang: "en",
            asset_name: "Phone-as-a-Key (PaaK) BLE / UWB Controller",
            interfaces: &["ble", "can"],
            properties: &["authenticity", "authorization"],
            threat_name: "Relay or spoof BLE authentication session to unlock vehicle",
            stride: "elevation-of-privilege",
            catalog_ids: &["5.1", "CAPEC-55"],
            damage_name: "Unauthorized physical entry and vehicle theft",
            impact: "financial:major",
            threat_proposals: vec![
                ThreatProposal {
                    name: "Relay or spoof BLE authentication session to unlock vehicle".into(),
                    stride: "elevation-of-privilege".into(),
                    catalog_ids: vec!["5.1".into(), "CAPEC-55".into()],
                    damage_name: "Unauthorized physical entry and vehicle theft".into(),
                    impact: "financial:major".into(),
                },
                ThreatProposal {
                    name: "Extract long-term BLE pairing keys from body control NVRAM".into(),
                    stride: "information-disclosure".into(),
                    catalog_ids: vec!["7.1".into(), "CAPEC-37".into()],
                    damage_name: "Cloning of owner digital key credentials".into(),
                    impact: "privacy:major".into(),
                },
            ],
            tree: gate(
                GateKind::Or,
                "Unlock and start the vehicle without the owner's authorization",
                vec![
                    gate(
                        GateKind::SeqAnd,
                        "Execute a real-time BLE link-layer relay attack",
                        vec![
                            step(
                                "Place relay node near the owner's smartphone",
                                "low",
                            ),
                            step(
                                "Forward GATT challenge-response frames to vehicle receiver",
                                "medium",
                            ),
                            step(
                                "Manipulate RSSI or time-of-flight bounding check",
                                "high",
                            ),
                        ],
                    ),
                    gate(
                        GateKind::SeqAnd,
                        "Exploit BLE stack vulnerability in the body controller",
                        vec![
                            step(
                                "Send malformed L2CAP advertising packets within range",
                                "low",
                            ),
                            step(
                                "Trigger buffer overflow in BLE host controller stack",
                                "high",
                            ),
                            step("Issue unlock frame onto the body CAN bus", "medium"),
                        ],
                    ),
                ],
            ),
        },
        SeedScenario {
            id: "bms-overcharge-en",
            methodology_key: "iso21434-r155",
            lang: "en",
            asset_name: "High-Voltage Battery Management System (BMS)",
            interfaces: &["can", "plc-ccs", "debug-port"],
            properties: &["integrity", "availability", "authenticity"],
            threat_name: "Tamper with cell voltage and thermal cutoff thresholds during DC fast charging",
            stride: "tampering",
            catalog_ids: &["4.2", "CAPEC-441", "EMB3D-TID-201"],
            damage_name: "Thermal runaway of high-voltage traction battery",
            impact: "safety:severe",
            threat_proposals: vec![
                ThreatProposal {
                    name: "Tamper with cell voltage and thermal cutoff thresholds during DC fast charging".into(),
                    stride: "tampering".into(),
                    catalog_ids: vec!["4.2".into(), "CAPEC-441".into()],
                    damage_name: "Thermal runaway of high-voltage traction battery".into(),
                    impact: "safety:severe".into(),
                },
                ThreatProposal {
                    name: "Spoof ISO 15118充电 session parameters from rogue EVSE charger".into(),
                    stride: "spoofing".into(),
                    catalog_ids: vec!["4.1".into(), "CAPEC-151".into()],
                    damage_name: "Overcurrent delivery exceeding pack limits".into(),
                    impact: "safety:severe".into(),
                },
            ],
            tree: gate(
                GateKind::Or,
                "Cause unsafe overcharging on the high-voltage BMS",
                vec![
                    gate(
                        GateKind::SeqAnd,
                        "Compromise PLC charging session from a modified charging station",
                        vec![
                            step("Connect vehicle to a modified DC fast charger", "low"),
                            step(
                                "Exploit parsing flaw in ISO 15118 V2G message handler",
                                "high",
                            ),
                            step(
                                "Override maximum charge current and voltage target",
                                "medium",
                            ),
                        ],
                    ),
                    gate(
                        GateKind::And,
                        "Disable independent thermal protection cutoff",
                        vec![
                            step(
                                "Modify calibration table in BMS flash via debug port",
                                "high",
                            ),
                            step(
                                "Suppress cell temperature sensor fault alerts on CAN",
                                "medium",
                            ),
                        ],
                    ),
                ],
            ),
        },
        SeedScenario {
            id: "adas-camera-ethernet-en",
            methodology_key: "iso21434-r155",
            lang: "en",
            asset_name: "ADAS Forward Perception Camera & Radar Fusion ECU",
            interfaces: &["ethernet", "can", "debug-port"],
            properties: &["integrity", "authenticity", "availability"],
            threat_name: "Inject forged SOME/IP object detection streams over Automotive Ethernet",
            stride: "spoofing",
            catalog_ids: &["4.1", "CAPEC-94", "EMB3D-TID-304"],
            damage_name: "Failure to detect obstacle or phantom emergency braking",
            impact: "safety:severe",
            threat_proposals: vec![
                ThreatProposal {
                    name: "Inject forged SOME/IP object detection streams over Automotive Ethernet"
                        .into(),
                    stride: "spoofing".into(),
                    catalog_ids: vec!["4.1".into(), "CAPEC-94".into()],
                    damage_name: "Failure to detect obstacle or phantom emergency braking".into(),
                    impact: "safety:severe".into(),
                },
                ThreatProposal {
                    name: "Disrupt gPTP precision time synchronization on ADAS Ethernet switch"
                        .into(),
                    stride: "denial-of-service".into(),
                    catalog_ids: vec!["4.3".into(), "CAPEC-125".into()],
                    damage_name: "Deactivation of sensor fusion and lane keeping assist".into(),
                    impact: "safety:major".into(),
                },
            ],
            tree: gate(
                GateKind::Or,
                "Feed falsified obstacle tracks to the ADAS fusion controller",
                vec![
                    gate(
                        GateKind::SeqAnd,
                        "Spoof SOME/IP camera object stream on Automotive Ethernet",
                        vec![
                            step(
                                "Attach tapping device to unencrypted 100BASE-T1 harness",
                                "medium",
                            ),
                            step(
                                "Bypass VLAN port isolation on the Ethernet switch",
                                "medium",
                            ),
                            step(
                                "Send forged SOME/IP service discovery and object packets",
                                "medium",
                            ),
                        ],
                    ),
                    gate(
                        GateKind::SeqAnd,
                        "Corrupt gPTP clock synchronization to desynchronize radar and camera",
                        vec![
                            step(
                                "Gain execution on a non-safety ECU connected to the switch",
                                "medium",
                            ),
                            step(
                                "Broadcast rogue grandmaster clock announce messages",
                                "low",
                            ),
                        ],
                    ),
                ],
            ),
        },
        SeedScenario {
            id: "jtag-hsm-extract-ja",
            methodology_key: "iso21434-r155",
            lang: "ja",
            asset_name: "ゲートウェイECU ハードウェアセキュリティモジュール (HSM)",
            interfaces: &["debug-port", "can"],
            properties: &["confidentiality", "integrity", "authenticity"],
            threat_name: "デバッグインターフェースと電圧グリッチを用いてHSM内の秘密鍵を抽出する",
            stride: "information-disclosure",
            catalog_ids: &["7.1", "CAPEC-191", "EMB3D-TID-101"],
            damage_name: "車載ネットワーク全体のSecOC署名鍵およびセキュアブート鍵の漏洩",
            impact: "operational:severe",
            threat_proposals: vec![ThreatProposal {
                name: "デバッグインターフェースと電圧グリッチを用いてHSM内の秘密鍵を抽出する"
                    .into(),
                stride: "information-disclosure".into(),
                catalog_ids: vec!["7.1".into(), "CAPEC-191".into()],
                damage_name:
                    "車載ネットワーク全体のSecOC署名鍵およびセキュアブート鍵の漏洩".into(),
                impact: "operational:severe".into(),
            }],
            tree: gate(
                GateKind::SeqAnd,
                "デバッグポートから鍵ストレージを読み出す",
                vec![
                    step("基板上のJTAGまたはSWDヘッダを特定する", "low"),
                    gate(
                        GateKind::Or,
                        "読み出し保護（Readout Protection）を突破する",
                        vec![
                            step(
                                "電源電圧フォールトインジェクションで保護判定をスキップする",
                                "high",
                            ),
                            step(
                                "ポートが有効なまま残っている工場テストモードへ遷移させる",
                                "medium",
                            ),
                        ],
                    ),
                    step("フラッシュおよび鍵領域のメモリをダンプする", "low"),
                ],
            ),
        },
    ]
}

/// Concrete slot bindings used to expand template items (`{bus}`, `{ecu}`, `{function}`).
#[must_use]
pub fn slot_binding_seeds() -> Vec<SlotBindingSeed> {
    vec![
        SlotBindingSeed {
            lang: "en",
            bus: "chassis CAN-FD bus",
            ecu: "Brake Control ECU",
            function: "emergency deceleration",
        },
        SlotBindingSeed {
            lang: "en",
            bus: "powertrain CAN bus",
            ecu: "Electric Power Steering (EPS) ECU",
            function: "steering torque overlay",
        },
        SlotBindingSeed {
            lang: "en",
            bus: "body LIN sub-bus",
            ecu: "Door Zone Controller",
            function: "central door unlock",
        },
        SlotBindingSeed {
            lang: "en",
            bus: "Automotive Ethernet VLAN 20",
            ecu: "ADAS Domain Controller",
            function: "autonomous lane change",
        },
        SlotBindingSeed {
            lang: "en",
            bus: "high-voltage powertrain CAN bus",
            ecu: "Battery Management System (BMS) ECU",
            function: "high-voltage contactor control",
        },
        SlotBindingSeed {
            lang: "en",
            bus: "telematics diagnostic CAN gateway bus",
            ecu: "Telematics Control Unit (TCU)",
            function: "remote immobilizer state transition",
        },
        SlotBindingSeed {
            lang: "ja",
            bus: "シャシーCAN-FDバス",
            ecu: "ブレーキ制御ECU",
            function: "緊急制動",
        },
        SlotBindingSeed {
            lang: "ja",
            bus: "パワートレインCANバス",
            ecu: "電動パワーステアリング (EPS) ECU",
            function: "操舵トルク制御",
        },
    ]
}

/// Non-TARA / general-world prompts that MUST map to `<|out_of_domain|>` so the
/// compact model refuses anything outside TARA and cybersecurity.
#[must_use]
pub fn out_of_domain_prompts() -> &'static [&'static str] {
    &[
        "Write a Python web scraper for e-commerce product prices.",
        "Give me a recipe for sourdough bread with hydration percentages.",
        "Write a haiku about autumn leaves in Kyoto.",
        "Explain the rules of offside in football.",
        "Summarize the plot of Hamlet in three paragraphs.",
        "How do I center a div in CSS Flexbox and Grid?",
        "Draft a marketing email for a summer shoe sale.",
        "Calculate the mortgage payment for a $400,000 house at 6% interest.",
        "Translate this restaurant menu into French.",
        "Write a SQL query to rank employees by quarterly sales.",
        "What is the capital of Australia and its population?",
        "Write a React hook for debouncing a search input field.",
        "Recommend a 5-day travel itinerary for Rome and Florence.",
        "Explain photosynthesis in simple terms for a middle school class.",
        "Create a workout plan for building marathon endurance.",
        "How do I bake chocolate chip cookies that stay chewy?",
        "Write a cover letter for a graphic designer job application.",
        "Explain the difference between stocks, bonds, and mutual funds.",
        "いしいラーメンのスープの作り方を教えてください。",
        "京都の紅葉についての俳句を詠んでください。",
        "ReactでToDoアプリを作成するコードを書いてください。",
        "明日の東京の天気予報を教えてください。",
    ]
}
