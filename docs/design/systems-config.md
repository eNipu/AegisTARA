# UX sketches: systems, and settings on ECUs and interfaces

- Status: accepted with decision D26 (one system per component) and
  [brainstorm-systems-config.md](../brainstorm-systems-config.md).
- Date: 2026-11-02
- Rules followed: [ui-ux-requirements.md](../ui-ux-requirements.md) section 2
  (modeless, create in the inspector, undo instead of confirmation, keyboard
  first) and UI-12 (no new modal). Every sketch below uses the existing page,
  inspector (right slot), left and bottom slots (UI-02) and the toast (UI-10).
- Text is shown in English with the Japanese string in the table at the end.
  Japanese runs about 1.0 to 1.3 times the English width at the UI font size;
  every label below has room for that or truncates with a tooltip.

Sketches are ASCII on purpose: they are for the builder and the UX review,
and they diff in a pull request.

---

## 1. The view switcher (systems in the top bar)

Only when the project has at least one system. With none, nothing changes.

```text
┌───────────────────────────────────────────────────────────────────────────────┐
│ ↶ ↷ │ E-platform 2028 ✎ │ View: [ Brake + ADAS ▾ ] │        ⌘K Search   👤 Ana │
└───────────────────────────────────────────────────────────────────────────────┘
```

Clicking the chip (or Tab to it, then Space or ↓) opens a dropdown anchored to
the chip. It is not a modal: it closes on Escape, on a click outside, or when
focus leaves, and the page stays live behind it.

```text
                    ┌──────────────────────────────┐
                    │ ◉ All systems           (51) │
                    │ ─────────────────────────────│
                    │ ☑ Brake                 (17) │
                    │ ☑ ADAS                  (20) │
                    │ ☐ Gateway                (2) │
                    │ ☐ Backend                (9) │
                    │ ─────────────────────────────│
                    │ ☐ Not in a system        (3) │
                    │ ─────────────────────────────│
                    │ Edit systems…   (item def.)  │
                    └──────────────────────────────┘
```

- Numbers are component counts. A component is in one system, so they add
  up to the "All systems" count.
- "All systems" is a radio: it clears the checks. Checking any system turns it
  off. Unchecking the last system goes back to "All systems".
- Keyboard: ↑ ↓ move, Space toggles, Enter closes, Escape closes and keeps the
  choice made so far (a view change is not an edit, so there is nothing to
  undo; the change is instant and harmless).
- The command palette (UI-01) has "View: All systems", "View: Brake",
  "View: add ADAS", so the view can be set without the pointer. No new global
  shortcut (none free on both platforms without clashes).
- The view is remembered per user and per project, like panel sizes. It is
  never an op and never in the audit log.
- The chip reads "Brake", "Brake + ADAS", "Brake +2", or "All systems";
  in a narrow window it shrinks to "Brake +2" first, then to an icon with the
  count and a tooltip.

## 2. Systems on the item definition page

Systems are edited where components live. Adding a system is a draft row
(UI-08): type the name, Enter creates it.

```text
┌ Item definition ─────────────────────────────────────────────────────────────┐
│ Systems                                                         [+ System]   │
│ ┌─────────┬──────────────────────────────┬────────────────┬───┐              │
│ │ Name    │ Components                   │ Boundary flows │   │              │
│ ├─────────┼──────────────────────────────┼────────────────┼───┤              │
│ │ Brake   │ ESC ECU, Wheel speed ×4      │ ⇄ Gateway 2    │ ⋯ │              │
│ │ ADAS    │ ADAS ECU, Front radar        │ ⇄ Gateway 3    │ ⋯ │              │
│ │ Gateway │ Gateway (GW), OBD connector  │ ⇄ 3 systems 6  │ ⋯ │              │
│ │ Backend │ OTA server, TCU              │ ⇄ Gateway 1    │ ⋯ │              │
│ │ ▌New system name…                      │                │   │ ← draft row  │
│ └─────────┴──────────────────────────────┴────────────────┴───┘              │
│                                                                              │
│ Components                                                   [+ Component]   │
│ ┌────┬───────────────┬──────────────┬────────────┬──────────────────────┬───┐│
│ │ ID │ Name          │ System       │ Interfaces │ Settings             │   ││
│ ├────┼───────────────┼──────────────┼────────────┼──────────────────────┼───┤│
│ │C01 │ Gateway (GW)  │ Gateway      │ 4          │ 🛡 Firewall, Sec.boot │ ⋯ ││
│ │C02 │ ADAS ECU      │ ADAS         │ 3          │ 🛡 Sec.boot +2        │ ⋯ ││
│ │C03 │ Front radar   │ ADAS         │ 1          │ —                    │ ⋯ ││
│ │C09 │ Diag tester   │ ⚠ none       │ 1          │ —                    │ ⋯ ││
│ └────┴───────────────┴──────────────┴────────────┴──────────────────────┴───┘│
│ Data flows   (boundary flows marked ⇄)                                      │
│ │ ADAS ECU → GW   "Brake request"   ⇄ ADAS → Gateway    2 threats      │   │
│ │ GW → ESC ECU    "Brake request"   ⇄ Gateway → Brake   2 threats      │   │
└──────────────────────────────────────────────────────────────────────────────┘
```

- "System" and "Settings" columns appear only when there is something to show
  (a project with no systems has no System column).
- The gateway is a system of its own because Brake, ADAS and Backend all use
  it. Moving it there from Brake is one edit: pick "Gateway" in its inspector
  (section 3); its interfaces, assets and threats move with it, and the toast
  says "Moved Gateway (GW) to Gateway: 4 interfaces, 3 assets, 7 threats
  moved. Undo".
- ⋯ on a system row: Rename, Remove. Remove happens at once and the toast says
  "Removed system Backend (2 components now in no system). Undo". No
  confirmation: removing a system removes no element.
- ⚠ none: the component is in no system; the Problems page lists it too.
- Bulk edit (UI-04) gains "Move to system ▾" in the bar for picked component
  rows (one batch, one undo step).

## 3. Component inspector: systems, interfaces, settings

```text
┌ Inspector ─────────────────────────────────┐
│ C02  ADAS ECU                         ⋯    │
│ Component                                  │
│                                            │
│ System    [ ADAS                      ▾ ]  │
│                                            │
│ Interfaces (3)                    [+ Add]  │
│  CAN-1        CAN        🛡 1              │
│  USB service  USB        —                 │
│  JTAG         Debug      🛡 Debug fused    │
│                                            │
│ Security settings (2)       [+ Add setting]│
│  🛡 Secure boot                         ⋯   │
│     firmware modification → Low            │
│     caps 3 steps · lowers 1 risk           │
│  🛡 Housing sealed                      ⋯   │
│     physical tampering → Low               │
│     caps 1 step · lowers no risk           │
│                                            │
│ Steps through this (6)              Show ▸ │
│ Description                                │
│ ┌────────────────────────────────────────┐ │
│ │ Domain controller for ADAS …           │ │
│ └────────────────────────────────────────┘ │
└────────────────────────────────────────────┘
```

- The System picker lists the project's systems and "No system"; for a child
  component it is read-only and says "In ADAS, with its parent ADAS ECU".
- "caps 3 steps · lowers 1 risk" is computed live; clicking it opens the risk
  table filtered to those risks (the filter chip says "Reduced by Secure boot
  on ADAS ECU", × clears it).
- ⋯ on a setting: Open setting (selects the control), Edit rules here, Remove
  from this component. Remove from this component happens at once with a toast
  that names what moved (section 8).
- "Steps through this ▸" opens the list in the bottom slot (UI-02) with
  tree, step, attack type, inherent, residual; Enter on a row opens that step on
  the canvas.

## 4. Interface inspector: own settings and settings from the ECU

```text
┌ Inspector ─────────────────────────────────┐
│ I07  JTAG                             ⋯    │
│ Interface of ADAS ECU · Debug              │
│                                            │
│ Security settings           [+ Add setting]│
│  🛡 Debug port fused                    ⋯   │
│     debug access → Very low (closed)       │
│ From ADAS ECU                              │
│  🛡 Secure boot          (applies here too) │
│     firmware modification → Low            │
│  🛡 Housing sealed       (applies here too) │
│                                            │
│ Steps through this (2)              Show ▸ │
└────────────────────────────────────────────┘
```

- Settings from the component are shown, dimmed, with the reason "applies here
  too". They are edited on the component (the row's link goes there).
- "(closed)" is the label for a ceiling at the pack's lowest level.

## 5. Add setting (presets and custom), modeless

"+ Add setting" opens an inline list under the button, inside the inspector,
with a filter field focused. Not a dialog.

```text
│ Security settings           [+ Add setting]│
│ ┌────────────────────────────────────────┐ │
│ │ 🔍 sec▌                                │ │
│ │ Existing in this project               │ │
│ │   Secure boot           (on 2 ECUs)    │ │
│ │ From the pack (ISO/SAE 21434)          │ │
│ │   Secure flash                         │ │
│ │   Secure diagnostics (seed-key)        │ │
│ │   Security access with HSM             │ │
│ │ ────────────────────────────────────── │ │
│ │   + Custom setting "sec"               │ │
│ └────────────────────────────────────────┘ │
```

- Existing: applies the same control to this element (one op). This is how
  one "Secure boot" covers two ECUs.
- From the pack: creates the control with the preset's rules and applies it
  (one batch, one undo step).
- Custom: creates a draft control in the inspector with the typed name and an
  empty rules table (section 6); Enter on the name creates it (UI-08).
- Presets that do not fit the element are not listed (a preset for debug
  interfaces does not show on a CAN interface).
- Keyboard: type to filter, ↑ ↓, Enter picks, Escape closes.

## 6. Setting (control) inspector, with rules

```text
┌ Inspector ─────────────────────────────────┐
│ K04  Seed-key with HSM                ⋯    │
│ Security setting · Custom                  │
│                                            │
│ Applied to (2)                     [+ Add] │
│  ADAS ECU / USB service             ×      │
│  ESC ECU / CAN-diag                 ×      │
│                                            │
│ Rules: when an attack step through these   │
│ targets is of type … its feasibility is at │
│ most …                                     │
│ ┌─────────────────────────┬────────────┬─┐ │
│ │ Attack type             │ At most    │ │ │
│ ├─────────────────────────┼────────────┼─┤ │
│ │ diagnostic access     ▾ │ Low      ▾ │×│ │
│ │ firmware modification ▾ │ Medium   ▾ │×│ │
│ │ + Add rule                           │ │ │
│ └─────────────────────────┴────────────┴─┘ │
│ caps 4 steps · lowers 2 risks              │
│                                            │
│ Also set by hand on 1 step          Show ▸ │
│ Goals: CG-03                               │
└────────────────────────────────────────────┘
```

- For a preset-based setting the subtitle says "Security setting · Secure boot
  (pack preset)"; if its rules differ from the pack's current values, a line
  says "Rules differ from the pack. [Use the pack's values]" (undoable).
- Each rule change is one op; the "caps · lowers" line updates at once.
- "Also set by hand" lists the manual step mitigations of today (ADR 0017),
  so the two ways are visible in one place.

## 7. Attack step: via, attack type, and why the number changed

Step inspector (a step selected on the canvas or in the outline):

```text
┌ Inspector ─────────────────────────────────┐
│ Flash modified firmware over USB      ⋯    │
│ Attack step in "Spoofed brake request"     │
│                                            │
│ Through   [ ADAS ECU / USB service    ▾ ]  │
│ Attack type [ firmware modification   ▾ ]  │
│                                            │
│ Feasibility                                │
│  Inherent   High   (attack potential 9)    │
│  Residual   Low                            │
│   capped by: Secure boot on ADAS ECU       │
│              (firmware modification → Low) │
│   also:      Seed-key with HSM on          │
│              ADAS ECU / USB service        │
│              (firmware modification →      │
│              Medium)                       │
│                                            │
│ [Factors ▸]  [Set by hand ▸]               │
└────────────────────────────────────────────┘
```

- The deciding ceiling is first; the others follow under "also".
- With no via or attack type, the residual block says "not capped" and a quiet
  hint: "Choose what this step goes through to let settings apply."
- The via picker lists components, then their interfaces indented, with
  type-to-filter; the attack type picker lists the pack's types.

On the canvas, a capped step shows a shield and both values; the step that
decides the threat stays highlighted as today:

```text
        ┌─────────────────────────────┐
        │ OR  Spoofed brake request   │  Residual Medium
        └──────┬───────────┬──────────┘
     ┌─────────┴───┐ ┌─────┴────────┐ ┌───────────────────┐
     │ Flash FW USB│ │ Keys via JTAG│ │ Inject on CAN-1   │ ◀ decides
     │ High → Low 🛡│ │ Med → V.low🛡 │ │ Medium            │
     │ ⟶ ADAS ECU  │ │ ⟶ ADAS/JTAG  │ │ ⟶ GW / OBD ·Gatew.│
     └─────────────┘ └──────────────┘ └───────────────────┘
```

- The shield has an accessible name: "Capped by Secure boot on ADAS ECU".
- "⟶ ADAS ECU" is the via chip (job 122); it truncates first in small nodes.
  When the via component is in another system than the tree's threat, the
  chip adds that system ("· Gateway"), so a Brake tree shows where it leaves
  Brake. The step is capped by the Gateway's settings in every view.

## 8. Seeing why a rating changed: the undo toast

Every settings change ends with the usual toast (UI-10.9), which also says
what moved:

```text
      ┌──────────────────────────────────────────────────────────────────┐
      │ Applied Secure boot to ADAS ECU: 3 steps capped, 1 risk lowered  │
      │ (T07 R4 → R3).                          [Show]  [Undo]  ×        │
      └──────────────────────────────────────────────────────────────────┘
```

- [Show] opens the risk table filtered to the risks that moved.
- Removing a setting says the opposite: "Removed Secure flash from ESC ECU:
  2 risks raised (T03 R2 → R3, T11 R1 → R2)". This is the "cost of dropping a
  measure" the consultant shows the client (story U6).
- The History panel (UI-03) lists the same sentence for the op, so it can be
  found later.

## 9. Risk table: systems and "reduced by"

```text
┌ Risks ── View: Brake + ADAS ─────────────────────────────────────────────────┐
│ ┌────┬──────────────────────────┬────────────┬──────┬──────┬────────────────┐│
│ │ ID │ Threat                   │ System     │ Inh. │ Res. │ Reduced by     ││
│ ├────┼──────────────────────────┼────────────┼──────┼──────┼────────────────┤│
│ │T07 │ Spoofed brake request    │ Brake      │ R4   │ R3   │ —  (CAN step)  ││
│ │T03 │ ESC firmware replaced    │ Brake      │ R4   │ R2   │ Secure flash   ││
│ │T12 │ Radar calibration altered│ ADAS       │ R3   │ R1   │ Debug fused +1 ││
│ └────┴──────────────────────────┴────────────┴──────┴──────┴────────────────┘│
└──────────────────────────────────────────────────────────────────────────────┘
```

- "Reduced by" names the settings on the deciding path. When nothing caps it,
  it names the step that decides ("CAN step"), which is where to act next.
- The same column is in the control table ("Lowers N risks") and in the XLSX
  risk register.

## 10. Combined view (overview with several systems)

```text
┌ Overview ── View: All systems ───────────────────────────────────────────────┐
│ ┌─────────┬────────────┬────────┬───────────────┬──────────────────────────┐ │
│ │ System  │ Components │ Risks  │ Highest res.  │ Boundary flows           │ │
│ ├─────────┼────────────┼────────┼───────────────┼──────────────────────────┤ │
│ │ Brake   │ 17         │ 22     │ R3            │ ⇄ Gateway 2              │ │
│ │ ADAS    │ 20         │ 29     │ R4            │ ⇄ Gateway 3              │ │
│ │ Gateway │ 2          │ 7      │ R3            │ ⇄ Brake, ADAS, Backend 6 │ │
│ │ Backend │ 9          │ 12     │ R2            │ ⇄ Gateway 1              │ │
│ │ No sys. │ 3          │ 0      │ —             │ ⚠ put them in a system   │ │
│ ├─────────┼────────────┼────────┼───────────────┼──────────────────────────┤ │
│ │ All     │ 51         │ 70     │ R4            │ 6                        │ │
│ └─────────┴────────────┴────────┴───────────────┴──────────────────────────┘ │
│ Boundary flows (6)                                                           │
│  ⇄ ADAS ECU → GW            "Brake request"         2 threats                │
│  ⇄ GW → ESC ECU             "Brake request"         2 threats                │
│  ⇄ TCU → GW                 "Update package"        0 threats  ⚠             │
│ Top attack paths in this view                                                │
│  1. Inject on CAN-1 via GW/OBD → Spoofed brake request (Medium)              │
└──────────────────────────────────────────────────────────────────────────────┘
```

- Components, assets, threats and risks are in one system each, so the rows
  add up to "All". A damage scenario or control used by two systems shows in
  both systems' pages; the overview counts it once in "All".
- ⚠ on a boundary flow with no threat, as the trust-boundary lint does.

## 11. Small window (900 × 600)

```text
┌ ↶ ↷ │ E-platform… │ View: Brake +1 ▾ │      ⌘K   👤 ┐
├────┬──────────────────────────────┬────────────────┤
│ ☰  │ Components                   │ Inspector      │
│ ▣  │ ID  Name        Settings     │ C02 ADAS ECU   │
│ ⚠  │ C01 Gateway     🛡 2          │ System ADAS ▾  │
│ ⛬  │ C02 ADAS ECU    🛡 3          │                │
│ …  │ C03 Front radar —            │ Settings (2) + │
│    │                              │  🛡 Secure boot │
│    │                              │  🛡 Housing…    │
└────┴──────────────────────────────┴────────────────┘
```

- The left page list collapses to icons (as today), the top bar drops the
  project name to an ellipsis before the view chip shrinks, and table columns
  drop in this order: Interfaces, System, then Settings becomes a count.
- The inspector keeps its minimum width (UI-02); long setting names truncate
  with a tooltip; rule rows wrap the attack type onto two lines in Japanese.

## 12. Keyboard map (new parts only)

| Where | Keys | Action |
|---|---|---|
| Top bar | Tab to the chip, Space or ↓ | open the view list |
| View list | ↑ ↓, Space, Enter, Escape | move, toggle, close, close |
| Command palette | "View: …" | set the view |
| Systems table | Enter on the draft row | create the system |
| Inspector | Tab to "+ Add setting", Enter | open the setting list |
| Setting list | type, ↑ ↓, Enter, Escape | filter, move, pick, close |
| Rules table | Tab between pickers, Delete on a row | edit, remove the rule (undo toast) |
| Step inspector | Tab to Through / Attack type, Space | open the picker |
| Toast | the existing undo shortcut | undo the change, as everywhere |

## 13. Text (English and Japanese)

Japanese follows the glossary of job 068; the pack labels come from the pack's
`ja` translations. The builder adds the keys to both `.po` catalogs and the
pseudo-locale (the i18n test fails otherwise).

| Key idea | English | Japanese |
|---|---|---|
| system | System | システム |
| all systems | All systems | すべてのシステム |
| view chip | View: | 表示: |
| not in a system | Not in a system | どのシステムにも属さない |
| no system (picker) | No system | システムなし |
| move to system | Move to system | システムへ移動 |
| boundary flow | Boundary flow | システム間フロー |
| security settings | Security settings | セキュリティ設定 |
| add setting | Add setting | 設定を追加 |
| custom setting | Custom setting | カスタム設定 |
| applied to | Applied to | 適用先 |
| applies here too | applies here too | ここにも適用 |
| rule | Rule | ルール |
| at most | At most | 上限 |
| through (via) | Through | 経由 |
| attack type | Attack type | 攻撃タイプ |
| steps through this | Steps through this | この要素を経由するステップ |
| capped by | capped by | 上限の理由 |
| not capped | not capped | 上限なし |
| closed | closed | 遮断 |
| reduced by | Reduced by | 低減策 |
| caps N steps · lowers M risks | caps {n} steps · lowers {m} risks | {n} 件のステップに上限 · {m} 件のリスクを低減 |
| rules differ from the pack | Rules differ from the pack | パックの既定値と異なるルール |
| use the pack's values | Use the pack's values | パックの値を使用 |
| toast after apply | Applied {setting} to {target}: {n} steps capped, {m} risks lowered | {target} に {setting} を適用: {n} 件のステップに上限、{m} 件のリスクを低減 |
