#!/usr/bin/env python3
"""
Step 2a: Fetch and compile open-source cybersecurity datasets from MITRE
(EMB3D STIX 2.0.1 and CAPEC STIX 2.1) into a normalized English TARA &
Attack-Tree catalog (`tools/aegis-ai/packs/open_cyber_en.json`) verified by
`aegis-model`.

Sources & Permissive Licenses (Decision D5 & D14 compliant):
  1. MITRE EMB3D(TM) Threat Model for Embedded Devices (STIX 2.1 v2.0.1)
     URL: https://emb3d.mitre.org/assets/emb3d-stix-2.0.1.json
     81 embedded device threats across Hardware, System Software, Application
     Software, and Networking, with CWE mappings and device properties.
  2. MITRE CAPEC(TM) Common Attack Pattern Enumeration and Classification (STIX 2.1)
     URL: https://raw.githubusercontent.com/mitre/cti/master/capec/2.1/stix-capec.json
     Embedded, hardware, communications, physical-security, and supply-chain
     attack patterns with multi-phase execution flows and prerequisites.
"""

from __future__ import annotations

import html
import json
import re
import urllib.request
from pathlib import Path

EMB3D_URL = "https://emb3d.mitre.org/assets/emb3d-stix-2.0.1.json"
CAPEC_URL = "https://raw.githubusercontent.com/mitre/cti/master/capec/2.1/stix-capec.json"

EMBEDDED_KEYWORDS = (
    "can",
    "bus",
    "firmware",
    "bootloader",
    "jtag",
    "uart",
    "spi",
    "i2c",
    "debug",
    "ecu",
    "embedded",
    "microcontroller",
    "automotive",
    "sensor",
    "actuator",
    "rtos",
    "memory",
    "flash",
    "dma",
    "clock",
    "voltage",
    "glitch",
    "side-channel",
    "power",
    "electromagnetic",
    "cryptographic",
    "key",
    "certificate",
    "signature",
    "replay",
    "spoof",
    "man-in-the-middle",
    "update",
    "ota",
    "downgrade",
    "rollback",
    "bluetooth",
    "wireless",
    "rf",
    "cellular",
    "gnss",
    "gps",
    "usb",
    "ethernet",
    "arp",
    "tcp",
    "udp",
    "buffer",
    "overflow",
    "privilege",
    "authentication",
    "bypass",
    "denial",
    "flood",
    "jamming",
    "fuzzing",
    "diagnostic",
)

WEB_ONLY_KEYWORDS = (
    "cookie",
    "browser",
    "xss",
    "cross-site",
    "clickjack",
    "sql injection",
    "phishing",
    "social engineering",
    "javascript",
    "wordpress",
    "php",
    "dom-based",
    "flash player",
    "silverlight",
    "active directory",
    "kerberos",
    "ldap",
    "smtp",
    "imap",
    "pop3",
    "spidering web",
)

TAG_RE = re.compile(r"<[^>]+>")
WS_RE = re.compile(r"\s+")
CWE_MD_RE = re.compile(r"\[(CWE-\d+):\s*([^\]]+)\]")
CWE_LEVEL_SUFFIX_RE = re.compile(r"\s*\((?:Base|Variant|Class|Pillar|Compound)\)\s*$", re.IGNORECASE)
PHASE_RE = re.compile(r"<h3>\s*([^<]+?)\s*</h3>(.*?)(?=<h3>|$)", re.DOTALL | re.IGNORECASE)
BOLD_STEP_RE = re.compile(r"<b>\s*([^<:]+?)\s*:?\s*</b>", re.IGNORECASE)


def clean_text(raw: str, max_len: int = 92) -> str:
    txt = html.unescape(TAG_RE.sub(" ", raw))
    txt = WS_RE.sub(" ", txt).strip(" -:;.,")
    txt = re.sub(r"^\[[^\]]+\]\s*:?\s*", "", txt)
    if ". " in txt:
        txt = txt.split(". ", 1)[0]
    if len(txt) > max_len:
        txt = txt[: max_len - 3].rsplit(" ", 1)[0]
    return txt.strip(" -:;.,")


def download_cached(url: str, dest: Path) -> dict:
    if not dest.exists():
        dest.parent.mkdir(parents=True, exist_ok=True)
        req = urllib.request.Request(url, headers={"User-Agent": "AegisTARA-AI/0.1"})
        with urllib.request.urlopen(req, timeout=60) as resp:
            data = resp.read()
        dest.write_bytes(data)
    return json.loads(dest.read_text(encoding="utf-8"))


def map_emb3d_maturity_to_feasibility(maturity: str, category: str) -> str:
    m = (maturity or "").lower()
    c = (category or "").lower()
    if c == "hardware":
        return "low" if "theoretical" in m or "proof" in m else "medium"
    if "observed" in m:
        return "high"
    if "theoretical" in m:
        return "low"
    return "medium"


def map_emb3d_category_to_tara(
    category: str, title: str, desc: str
) -> tuple[str, list[str], str, str, str, str, list[tuple[str, str]]]:
    """
    Maps EMB3D threat category & keywords to:
      (asset_name, interfaces, stride, r155_id, damage_name, impact, canonical_access_steps)
    """
    text = f"{title} {desc}".lower()
    cat = (category or "").lower()

    if cat == "hardware":
        asset = "Zone Controller & HSM Hardware Module"
        ifaces = ["debug-port", "can"]
        access_steps = [
            ("Attach logic probe to ECU PCB test points or power rail", "low"),
            ("Connect to JTAG/SWD debug header on target controller", "low"),
        ]
        if any(k in text for k in ("side channel", "power", "electromagnetic", "rom", "read", "extract", "secret")):
            stride = "information-disclosure"
            r155 = "6.1"
            damage = "Extraction of cryptographic root keys or confidential firmware"
            impact = "privacy:major"
        elif any(k in text for k in ("glitch", "fault", "inject", "dma", "bus", "modify", "interposer")):
            stride = "tampering"
            r155 = "5.2"
            damage = "Unauthorized hardware state modification or secure boot bypass"
            impact = "safety:severe"
        else:
            stride = "elevation-of-privilege"
            r155 = "5.2"
            damage = "Bypass of hardware trust anchor on safety controller"
            impact = "safety:severe"

    elif cat == "system software":
        asset = "ECU Bootloader & RTOS System Firmware"
        ifaces = ["ota", "can", "debug-port"]
        access_steps = [
            ("Stage crafted firmware payload over OTA or UDS programming session", "medium"),
            ("Gain local diagnostic session on ECU bootloader interface", "medium"),
        ]
        if any(k in text for k in ("update", "bootloader", "firmware", "rom", "rootkit", "image", "driver")):
            stride = "tampering"
            r155 = "3.1"
            damage = "Boot of untrusted or tampered ECU system firmware image"
            impact = "safety:severe"
        elif any(k in text for k in ("exhaust", "hang", "crash", "denial", "watchdog", "resource", "interrupt")):
            stride = "denial-of-service"
            r155 = "4.3"
            damage = "Loss of real-time ECU control loop responsiveness"
            impact = "safety:severe"
        elif any(k in text for k in ("privilege", "kernel", "sandbox", "escape", "hypervisor")):
            stride = "elevation-of-privilege"
            r155 = "5.1"
            damage = "Execution of unprivileged task with RTOS kernel privileges"
            impact = "operational:major"
        else:
            stride = "information-disclosure"
            r155 = "6.1"
            damage = "Disclosure of RTOS kernel memory and cryptographic secrets"
            impact = "privacy:major"

    elif cat == "application software":
        asset = "Diagnostic & Infotainment Application Controller"
        ifaces = ["ethernet", "obd", "cellular"]
        access_steps = [
            ("Send crafted diagnostic or IPC request over Automotive Ethernet", "high"),
            ("Reach exposed application service via OBD-II or telematics interface", "medium"),
        ]
        if any(k in text for k in ("overflow", "memory", "inject", "command", "execution", "format", "pointer")):
            stride = "elevation-of-privilege"
            r155 = "5.1"
            damage = "Arbitrary code execution in vehicle application service"
            impact = "safety:major"
        elif any(k in text for k in ("auth", "credential", "password", "session", "token", "access")):
            stride = "spoofing"
            r155 = "4.1"
            damage = "Unauthorized invocation of privileged diagnostic service"
            impact = "operational:major"
        elif any(k in text for k in ("crypt", "key", "random", "entropy", "leak", "sensitive", "cleartext")):
            stride = "information-disclosure"
            r155 = "6.1"
            damage = "Exposure of vehicle identity credentials or session keys"
            impact = "privacy:major"
        else:
            stride = "tampering"
            r155 = "7.1"
            damage = "Corruption of vehicle calibration or application parameters"
            impact = "operational:major"

    else:  # networking
        asset = "Central Vehicle Gateway & Ethernet/CAN Switch"
        ifaces = ["ethernet", "can", "cellular"]
        access_steps = [
            ("Inject frames onto in-vehicle Ethernet or CAN segment", "medium"),
            ("Establish network session with vehicle gateway interface", "high"),
        ]
        if any(k in text for k in ("dos", "flood", "denial", "jam", "exhaust", "storm")):
            stride = "denial-of-service"
            r155 = "4.3"
            damage = "Loss of inter-domain gateway routing and safety telemetry"
            impact = "safety:severe"
        elif any(k in text for k in ("mitm", "man-in-the-middle", "intercept", "sniff", "eavesdrop", "tap")):
            stride = "information-disclosure"
            r155 = "6.1"
            damage = "Interception of internal vehicle network traffic"
            impact = "privacy:major"
        elif any(k in text for k in ("replay", "spoof", "false", "masquerad", "arp", "routing")):
            stride = "spoofing"
            r155 = "4.1"
            damage = "Acceptance of forged control messages across vehicle domains"
            impact = "safety:severe"
        else:
            stride = "tampering"
            r155 = "5.1"
            damage = "Unauthorized routing table or network session manipulation"
            impact = "operational:major"

    return asset, ifaces, stride, r155, damage, impact, access_steps


def canonical_cwe_step(text_lower: str, stride: str) -> str:
    """Maps CWE descriptions/titles and STRIDE into canonical embedded weakness steps."""
    if any(k in text_lower for k in ("side channel", "power", "electromagnetic", "glitch", "fault", "physical", "1300", "1240", "1255")):
        return "Exploit physical side-channel or fault-injection weakness (CWE-1300/CWE-1240)"
    if any(k in text_lower for k in ("jtag", "debug", "test", "dma", "bus", "register", "1191", "1262")):
        return "Exploit hardware debug or untrusted bus interface (CWE-1191/CWE-1262)"
    if any(k in text_lower for k in ("buffer", "overflow", "underflow", "out-of-bounds", "pointer", "memory", "119", "120", "787", "416")):
        return "Exploit memory buffer or pointer bounds weakness (CWE-119/CWE-787)"
    if any(k in text_lower for k in ("firmware", "update", "integrity", "download", "cleartext", "key", "random", "entropy", "494", "312", "330")):
        return "Exploit insecure firmware update or cryptographic storage (CWE-494/CWE-312)"
    if any(k in text_lower for k in ("exhaust", "resource", "flood", "denial", "race", "deadlock", "lock", "400", "362")):
        return "Exploit resource exhaustion or timing state weakness (CWE-400/CWE-362)"
    if any(k in text_lower for k in ("auth", "credential", "signature", "replay", "spoof", "session", "token", "287", "347", "290", "294")):
        return "Exploit improper authentication or cryptographic check (CWE-287/CWE-347)"
    if any(k in text_lower for k in ("privilege", "permission", "access control", "sandbox", "isolation", "284", "269", "862")):
        return "Exploit missing access control or privilege boundary (CWE-284/CWE-269)"
    if stride == "denial-of-service":
        return "Exploit resource exhaustion or timing state weakness (CWE-400/CWE-362)"
    if stride == "spoofing":
        return "Exploit improper authentication or cryptographic check (CWE-287/CWE-347)"
    return "Exploit improper input or protocol frame validation (CWE-20/CWE-1284)"


def extract_cwe_labels(cwe_markdown: str, title: str, stride: str) -> list[tuple[str, str]]:
    """Returns [(primary_cwe_id, canonical_cwe_step_label)] from EMB3D CWE Markdown."""
    cwe_matches = CWE_MD_RE.findall(cwe_markdown or "")
    primary_id = cwe_matches[0][0] if cwe_matches else "CWE-20"
    combined = f"{title} {cwe_markdown or ''}".lower()
    step_label = canonical_cwe_step(combined, stride)
    return [(primary_id, step_label)]


def extract_emb3d_scenarios(bundle: dict) -> list[dict]:
    objects = bundle.get("objects", [])
    threats = [o for o in objects if o.get("type") == "vulnerability"]

    scenarios = []
    for tobj in sorted(threats, key=lambda o: o.get("x_mitre_emb3d_threat_id", "")):
        ext_id = tobj.get("x_mitre_emb3d_threat_id", "").strip()
        if not ext_id:
            continue
        title = clean_text(tobj.get("name", ""), 72)
        desc = tobj.get("description", "")
        category = tobj.get("x_mitre_emb3d_threat_category", "networking")
        maturity = tobj.get("x_mitre_emb3d_threat_maturity", "proof of concept")
        exploit_feas = map_emb3d_maturity_to_feasibility(maturity, category)

        asset, ifaces, stride, r155, damage, impact, access_steps = map_emb3d_category_to_tara(
            category, title, desc
        )

        cwe_pairs = extract_cwe_labels(
            tobj.get("x_mitre_emb3d_threat_CWEs", ""), title, stride
        )
        catalog_ids = [r155, ext_id] + [cid for cid, _ in cwe_pairs[:1]]

        exploit_children = [
            {
                "label": f"Trigger {title} condition on {asset}",
                "gate": None,
                "feasibility": exploit_feas,
                "children": [],
            },
            {
                "label": cwe_pairs[0][1],
                "gate": None,
                "feasibility": exploit_feas,
                "children": [],
            },
        ]

        tree = {
            "label": f"Execute {title} ({ext_id})",
            "gate": "seq_and",
            "feasibility": None,
            "children": [
                {
                    "label": f"Reach {asset} attack surface",
                    "gate": "or",
                    "feasibility": None,
                    "children": [
                        {"label": s_label, "gate": None, "feasibility": s_feas, "children": []}
                        for s_label, s_feas in access_steps
                    ],
                },
                {
                    "label": f"Exploit {title} weakness ({ext_id})",
                    "gate": "or",
                    "feasibility": None,
                    "children": exploit_children,
                },
                {
                    "label": f"Cause {damage.lower()}",
                    "gate": None,
                    "feasibility": "high",
                    "children": [],
                },
            ],
        }

        scenarios.append(
            {
                "id": f"emb3d-{ext_id.lower()}",
                "source": "MITRE-EMB3D-2.0.1",
                "methodology_key": "iso21434-r155",
                "lang": "en",
                "asset_name": asset,
                "interfaces": ifaces,
                "properties": ["integrity", "authenticity", "confidentiality"],
                "threat_name": f"{title} on {asset}",
                "stride": stride,
                "catalog_ids": catalog_ids,
                "damage_name": damage,
                "impact": impact,
                "tree": tree,
            }
        )
    return scenarios


def map_capec_likelihood(val: str) -> str:
    v = (val or "").lower()
    if "high" in v:
        return "high"
    if "low" in v:
        return "low"
    return "medium"


def classify_capec_target(text_lower: str) -> tuple[str, list[str], str, str, str, str, list[tuple[str, str]]]:
    if any(k in text_lower for k in ("jtag", "debug", "glitch", "side-channel", "physical", "hardware", "power", "silicon")):
        return (
            "Zone Controller & HSM Hardware Module",
            ["debug-port", "can"],
            "information-disclosure" if any(w in text_lower for w in ("disclos", "extract", "side", "read", "probe")) else "tampering",
            "5.2",
            "Extraction of cryptographic root keys or confidential firmware",
            "safety:severe",
            [
                ("Attach logic probe to ECU PCB test points or power rail", "low"),
                ("Connect to JTAG/SWD debug header on target controller", "low"),
            ],
        )
    if any(k in text_lower for k in ("firmware", "bootloader", "update", "flash", "malicious logic", "integrity", "signature")):
        return (
            "ECU Bootloader & RTOS System Firmware",
            ["ota", "can", "debug-port"],
            "tampering",
            "3.1",
            "Boot of untrusted or tampered ECU system firmware image",
            "safety:severe",
            [
                ("Stage crafted firmware payload over OTA or UDS programming session", "medium"),
                ("Gain local diagnostic session on ECU bootloader interface", "medium"),
            ],
        )
    if any(k in text_lower for k in ("bluetooth", "wireless", "cellular", "rf", "jamming", "wi-fi", "gps", "gnss", "signal")):
        return (
            "Telematics & Short-Range Wireless Module",
            ["ble", "cellular", "v2x"],
            "denial-of-service" if any(w in text_lower for w in ("jam", "flood", "disrupt", "block")) else "spoofing",
            "4.1",
            "Spoofing or disruption of vehicle wireless communication link",
            "safety:major",
            [
                ("Position SDR transceiver within wireless range of vehicle antenna", "medium"),
                ("Capture and analyze wireless protocol handshake frames", "high"),
            ],
        )
    if any(k in text_lower for k in ("flood", "denial", "exhaust", "amplification", "resource", "starvation", "deadlock")):
        return (
            "Central Vehicle Gateway & Ethernet/CAN Switch",
            ["ethernet", "can", "cellular"],
            "denial-of-service",
            "4.3",
            "Loss of inter-domain gateway routing and safety telemetry",
            "safety:severe",
            [
                ("Inject frames onto in-vehicle Ethernet or CAN segment", "medium"),
                ("Establish network session with vehicle gateway interface", "high"),
            ],
        )
    if any(k in text_lower for k in ("buffer", "overflow", "privilege", "injection", "command", "memory", "pointer", "integer")):
        return (
            "Diagnostic & Infotainment Application Controller",
            ["ethernet", "obd", "cellular"],
            "elevation-of-privilege",
            "5.1",
            "Arbitrary code execution in vehicle application service",
            "safety:major",
            [
                ("Send crafted diagnostic or IPC request over Automotive Ethernet", "high"),
                ("Reach exposed application service via OBD-II or telematics interface", "medium"),
            ],
        )
    return (
        "Central Vehicle Gateway & Ethernet/CAN Switch",
        ["ethernet", "can", "cellular"],
        "spoofing" if any(w in text_lower for w in ("spoof", "masquerad", "replay", "impersonat", "man-in-the-middle")) else "tampering",
        "4.1",
        "Acceptance of forged control messages across vehicle domains",
        "safety:severe",
        [
            ("Inject frames onto in-vehicle Ethernet or CAN segment", "medium"),
            ("Establish network session with vehicle gateway interface", "high"),
        ],
    )


def extract_capec_scenarios(bundle: dict, max_items: int = 85) -> list[dict]:
    objects = bundle.get("objects", [])
    scenarios = []

    for obj in objects:
        if obj.get("type") != "attack-pattern" or obj.get("revoked") or obj.get("x_capec_status") == "Deprecated":
            continue

        capec_id = ""
        cwe_ids: list[str] = []
        for ref in obj.get("external_references", []):
            if ref.get("source_name") == "capec":
                capec_id = ref.get("external_id", "")
            elif ref.get("source_name") == "cwe":
                cid = ref.get("external_id", "")
                if cid and cid not in cwe_ids:
                    cwe_ids.append(cid)
        if not capec_id:
            continue

        name = clean_text(obj.get("name", ""), 68)
        desc = obj.get("description", "")
        flows_raw = obj.get("x_capec_execution_flow", "") or ""
        domains = set(obj.get("x_capec_domains", []) or [])

        combined_lower = f"{name} {desc}".lower()
        if any(w in combined_lower for w in WEB_ONLY_KEYWORDS):
            continue
        has_embedded_domain = bool(
            domains.intersection({"Hardware", "Communications", "Physical Security", "Supply Chain"})
        )
        if not (has_embedded_domain or any(kw in combined_lower for kw in EMBEDDED_KEYWORDS)):
            continue

        phases = PHASE_RE.findall(flows_raw)
        if not phases:
            continue

        feas = map_capec_likelihood(obj.get("x_capec_likelihood_of_attack", "Medium"))
        asset, ifaces, stride, r155, damage, impact, access_steps = classify_capec_target(combined_lower)

        cwe_step = canonical_cwe_step(f"{combined_lower} {' '.join(cwe_ids)}", stride)
        exploit_children = [
            {
                "label": f"Enumerate and probe exposed surface on {asset}",
                "gate": None,
                "feasibility": "high",
                "children": [],
            },
            {
                "label": f"Execute {name} technique ({capec_id})",
                "gate": None,
                "feasibility": feas,
                "children": [],
            },
            {
                "label": cwe_step,
                "gate": None,
                "feasibility": feas,
                "children": [],
            },
        ]

        tree = {
            "label": f"Execute {name} ({capec_id})",
            "gate": "seq_and",
            "feasibility": None,
            "children": [
                {
                    "label": f"Reach {asset} attack surface",
                    "gate": "or",
                    "feasibility": None,
                    "children": [
                        {"label": s_label, "gate": None, "feasibility": s_feas, "children": []}
                        for s_label, s_feas in access_steps
                    ],
                },
                {
                    "label": f"Exploit {name} attack pattern ({capec_id})",
                    "gate": "and",
                    "feasibility": None,
                    "children": exploit_children[:3],
                },
                {
                    "label": f"Cause {damage.lower()}",
                    "gate": None,
                    "feasibility": "high",
                    "children": [],
                },
            ],
        }

        catalog_ids = [r155, capec_id] + cwe_ids[:1]
        scenarios.append(
            {
                "id": f"capec-{capec_id.lower()}",
                "source": "MITRE-CAPEC-STIX-2.1",
                "methodology_key": "iso21434-r155",
                "lang": "en",
                "asset_name": asset,
                "interfaces": ifaces,
                "properties": ["integrity", "authenticity", "availability"],
                "threat_name": f"{name} on {asset}",
                "stride": stride,
                "catalog_ids": catalog_ids,
                "damage_name": damage,
                "impact": impact,
                "tree": tree,
            }
        )
        if len(scenarios) >= max_items:
            break

    return scenarios


def main() -> None:
    cache_dir = Path("tools/aegis-ai/data/raw_open_catalogs")
    out_file = Path("tools/aegis-ai/packs/open_cyber_en.json")

    emb3d_bundle = download_cached(EMB3D_URL, cache_dir / "emb3d-stix-2.0.1.json")
    capec_bundle = download_cached(CAPEC_URL, cache_dir / "stix-capec.json")

    emb3d_scenarios = extract_emb3d_scenarios(emb3d_bundle)
    capec_scenarios = extract_capec_scenarios(capec_bundle, max_items=85)

    combined = {
        "schema_version": 1,
        "attribution": (
            "Derived from MITRE EMB3D(TM) v2.0.1 and MITRE CAPEC(TM) STIX 2.1. "
            "(c) 2026 The MITRE Corporation. Reproduced and distributed with the "
            "permission of The MITRE Corporation."
        ),
        "emb3d_count": len(emb3d_scenarios),
        "capec_count": len(capec_scenarios),
        "scenarios": emb3d_scenarios + capec_scenarios,
    }

    out_file.parent.mkdir(parents=True, exist_ok=True)
    out_file.write_text(json.dumps(combined, indent=2), encoding="utf-8")
    print(
        f"ok    fetched and compiled {len(emb3d_scenarios)} MITRE EMB3D threats + "
        f"{len(capec_scenarios)} MITRE CAPEC execution-flow trees -> {out_file}"
    )


if __name__ == "__main__":
    main()
