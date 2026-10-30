# UI and UX requirements: platform fidelity and modeless editing

- Status: proposed; the owner can change any line.
- Date: 2026-10-30
- Requirement IDs: UI-07 to UI-12 (the UI table in [requirements.md](requirements.md) section 2.8).
- Jobs: 113 to 117 in [factory/backlog.md](../factory/backlog.md), Phase 5.
- Decision: D25 in [decisions.md](decisions.md) (stay on Slint; no native rewrite).
- Related jobs already planned: 081 command palette, 082 panel slots, 083 history
  panel, 084 bulk edit, 085 design system pass. This document does not repeat them.
  It says where they meet the new work.

---

## 1. The problem

The owner's own words: the app "looks not solid" on macOS, and modal windows
get in the way. For example, adding an asset opens a dialog that asks only for
a name.

A design review of `crates/aegis-shell` (2026-10-30) found the causes.

| # | Finding | Evidence | Effect on the user |
|---|---|---|---|
| F1 | Every platform uses the CPU (software) renderer | `src/main.rs` `BackendSelector ... renderer_name("software")` | Text is softer and thinner than in native Mac apps on Retina screens; scrolling and resizing are less smooth. This is most of "not solid". |
| F2 | No widget style is chosen, so macOS gets Slint's Fluent (Windows) style | `build.rs` has no `with_style`; `Cargo.toml` has no style feature | Buttons, fields, checkboxes and scroll bars look like Windows controls on a Mac |
| F3 | Dark mode is forced at start | `app.slint` `init => { Palette.color-scheme = ColorScheme.dark; }` | The app ignores the system appearance; it feels like a foreign app in a light-mode Mac |
| F4 | Shortcut texts are written as `Ctrl+...` | the keyboard shortcuts list in `app.slint` (`"Ctrl+Z / Ctrl+Y"`) | Wrong on macOS, where users expect ⌘ |
| F5 | Ten "Add" or "Create" actions open a one-line name dialog | `root.ask("add-asset", ...)` and the same for damage, tree, control, goal, claim, component, trust boundary, baseline, project | A full stop of the flow for one word; the dialog covers the context the user is naming |
| F6 | Three different ways to create things | dialogs (F5); inline forms for threats, interfaces and data flows; inline node fields in the attack tree page | The user must learn three patterns for one job |
| F7 | Renaming needs a "Rename" button or Enter | `NameEditor` in `app.slint` | Typing a new name and clicking elsewhere silently loses the change |
| F8 | Removing anything asks "Are you sure?" though undo exists | `root.confirm("remove-asset", ...)`, `Undo` in the Edit menu | Every delete costs two clicks and a read, and the dialog trains users to click "Yes" without reading |
| F9 | The attack tree library is a modal dialog, but inserting needs a selection on the canvas | library dialog text: "Select a gate of the tree on the canvas first." | The user cannot do what the dialog asks while the dialog is open |
| F10 | The description editor and outline editor are modal | `description-open`, `outline-open` overlays | The user cannot see the element or the tree while writing about it |
| F11 | Tables are `StandardTableView`, with no per-row actions | `Table` in `widgets.slint` | No "+" on a row to add a child; no hover or focus actions |

None of these needs a new UI toolkit. They are choices inside the current app.

---

## 2. Design principles

These principles decide every open detail in this document and in later UI jobs.
When a job breaks one, its UX review fails.

1. **Modeless by default.** Work happens in the page, the inspector (right
   panel) and the canvas. A dialog is allowed only for the cases in UI-12.
2. **Create in context, finish in the inspector.** Where the user is looking
   is where the new thing appears. Its details are filled in the right panel
   that already exists on every page.
3. **Undo is the safety net, not confirmation.** Anything undoable happens at
   once and offers Undo. A confirmation is kept only for what undo cannot bring
   back.
4. **Never lose typing.** Leaving a field saves it. Only Escape throws away.
5. **One selection, one inspector.** Selecting a row or a canvas node always
   shows that element in the same right panel, with the same layout.
6. **Feel at home on each platform.** macOS behaves like a Mac app (⌘, system
   appearance, Mac controls); Windows behaves like a Windows app.
7. **Keyboard first, pointer friendly.** Every action can be done from the
   keyboard; every hover-only control is also visible on keyboard focus.
8. **Nothing in the audit trail the user did not mean.** A half-made element
   is never sent to the server. This matters more in a TARA tool than in a
   notes app, because the op log is evidence for auditors.

---

## 3. Requirements

Priority: **M** must, **S** should, **C** could.

### UI-07 Platform fidelity (M, job 113)

The app must look and feel solid on macOS and Windows with no change to its
layout or features.

| ID | Requirement | Pri |
|---|---|---|
| UI-07.1 | Use a GPU renderer on desktops (Skia preferred; FemtoVG compared in the spike), with an automatic fallback to the software renderer when the GPU renderer cannot start (virtual desktops, remote sessions, old drivers). An `AEGIS_RENDERER=software` setting forces the fallback. | M |
| UI-07.2 | Before 07.1 is merged, a spike compares software, Skia and FemtoVG on one Retina Mac and one Windows laptop at 100 % and 150 % scale: screenshots of the assets page, the attack tree canvas and a Japanese page, the binary size, cold start time and memory. The owner picks from the screenshots. The result is recorded in an ADR. | M |
| UI-07.3 | The renderer passes D5 and `cargo-deny`, and builds without network access for the offline bundle (job 100). If Skia's build download breaks the offline build, vendor the prebuilt archive or choose FemtoVG. | M |
| UI-07.4 | The widget style is chosen per target OS at build time: `cupertino` on macOS, `fluent` on Windows and Linux (`CompilerConfiguration::with_style`, reading `CARGO_CFG_TARGET_OS` in `build.rs`). `Theme` colors stay on top. | M |
| UI-07.5 | The app follows the system light or dark appearance by default, and changes live when the system changes. A setting offers System, Light and Dark. The brand surfaces (splash, About) stay obsidian in both. | S |
| UI-07.6 | On macOS every shortcut uses ⌘ (and ⌥ for Alt), in the menus, tooltips, the shortcut list and the command palette (081). The shortcut list is built from one table, not from hand-written `Ctrl+` text. | M |
| UI-07.7 | The UI font is the system UI font (SF Pro on macOS, Segoe UI Variable on Windows), with Hiragino Sans and Yu Gothic UI for Japanese. Monospace text uses the system monospace font (SF Mono, Cascadia Mono). | S |
| UI-07.8 | On macOS the menu bar is the system menu bar with the standard App menu (About, Settings ⌘,, Hide, Quit ⌘Q), if Slint's `MenuBar` supports it on the chosen backend. If not, record the gap in the ADR; do not build a workaround. | C |

**Done when:** a side-by-side screenshot of the assets page next to a native Mac
app (for example Finder or Notes) at the same scale shows text of the same
crispness; the screenshot tests (`src/headless.rs`) still pass on the software
renderer; a forced GPU failure starts the app on the software renderer with one
log line and no error dialog.

### UI-08 Modeless creation (M, job 114)

One pattern for creating every element: a **draft in the inspector**.

**The flow (the asset example):**

1. The user clicks **+ New asset** in the page header, presses the page's new
   shortcut (proposed ⌘/Ctrl+Shift+N), or clicks **+** on a row (UI-09).
2. The table shows a draft row at the right place (end of the list, or under
   the parent row) in italic grey text: "New asset". The row is selected.
3. The inspector opens in draft mode: the name field is focused and empty, with
   the placeholder "Asset name". Fields that come from the context are already
   filled (for example the component, when created from a component row).
4. The user types the name.
   - **Enter** or **Tab** creates the element (one op, sent to the server). The
     draft row becomes a normal row. Tab moves focus to the next field.
   - **⌘/Ctrl+Enter** creates it and starts the next draft at once, for fast
     entry of many elements during a workshop.
   - **Clicking anywhere else** creates it if the name is not empty.
   - **Escape**, or leaving with an empty name, removes the draft. Nothing is
     sent and nothing is in the history.
5. A draft never reaches the server. Undo after creation removes the element as
   one step.

| ID | Requirement | Pri |
|---|---|---|
| UI-08.1 | The draft flow above for: asset, damage scenario, threat scenario, attack tree, security control, goal, claim, component, interface, data flow, trust boundary. It replaces the `ask()` dialog (F5) and the inline "Add a ..." forms (F6) on these pages. | M |
| UI-08.2 | Required fields beyond the name (a threat's asset and STRIDE category; an interface's component and kind; a flow's ends) are filled from the context when possible, otherwise shown in the draft inspector with the first required one focused after the name. The element is created only when all required fields have a value; until then the draft row shows a small "incomplete" mark. | M |
| UI-08.3 | Names are saved on Enter, Tab and focus loss everywhere (`NameEditor` and every name field). The "Rename" button is removed. Escape puts back the saved name. An empty name is never saved; the field shows the saved name again with a short inline note "A name is needed". | M |
| UI-08.4 | The Japanese IME works in the draft: Enter that confirms an IME composition does not create the element. Checked on macOS (Japanese – Romaji, Kana) and Windows (Microsoft IME). | M |
| UI-08.5 | A user who may not edit (viewer, or no lease) does not see the + buttons on rows; the page header button is shown disabled with a tooltip that says why ("Read only: Hana Suzuki is editing"). | M |
| UI-08.6 | Creation feels instant: the draft row and inspector appear within one frame; the created row appears at once (optimistic, as the client core does today) and shows an error inline in the inspector if the server refuses it, with the user's text kept. | M |
| UI-08.7 | New project and New project folder stay on the start screen as an inline card with a name field and Create button, not as a dialog. | S |

**Done when:** each element in 08.1 can be created with the keyboard alone, with
no dialog; the headless tests create, cancel (Escape) and create-and-next for
assets, threats and controls, and check that a cancelled draft sends no op; the
UX review records the time and the number of key presses to add five assets
before and after.

### UI-09 Row actions in tables (S, job 115)

| ID | Requirement | Pri |
|---|---|---|
| UI-09.1 | A `DataTable` widget in `widgets.slint` replaces the wrapper around `StandardTableView`, with the same columns and rows API so pages change little. Built once, so multi-select (084) and the density mode (085) build on it. | M |
| UI-09.2 | Each row shows two actions on the right on hover, on selection and on keyboard focus: **+** (add a child, where the row type has one) and **⋯** (a menu: Open, Duplicate where the model allows it, Remove). They are real buttons with accessible names ("Add asset to Brake ECU"). | M |
| UI-09.3 | What **+** adds, by row type (names come from the pack, D24): component → asset bound to it; asset → threat scenario on it; damage scenario → threat scenario that leads to it; threat scenario → attack tree for it (the existing "new tree for threat"); security control → goal; attack tree node → child node (the canvas keeps its own controls). A row type with no child shows no +. | M |
| UI-09.4 | Keyboard: arrow keys move the selection; Enter moves focus into the inspector; ⌘/Ctrl+Enter on a row adds a child (same as +); Delete or Backspace removes (UI-10); ⌘/Ctrl+D duplicates where allowed. | M |
| UI-09.5 | Rows are 28 px in compact density and 32 px in normal density (085); the header stays visible while scrolling; long names end with "…" and show the full name in a tooltip. | S |
| UI-09.6 | The table stays fast at the large reference project: scrolling 5,000 rows holds 60 frames per second on the GPU renderer and the selection change stays inside the A1 budget. | M |

### UI-10 Undo instead of "Are you sure?" (M, job 116)

| ID | Requirement | Pri |
|---|---|---|
| UI-10.1 | Removing an element, unlinking, and every other undoable change happen at once, with no confirmation dialog. | M |
| UI-10.2 | A toast at the bottom center says what happened and offers Undo: "Asset 'Brake ECU' removed · Undo (⌘Z)". It stays 8 seconds, longer while the pointer is over it, and never covers the inspector buttons. A new toast replaces the old one. It is read by screen readers (polite live region, or the accessible label route used in job 051). | M |
| UI-10.3 | When a removal also changes other elements (unlinks threats, orphans a tree), the toast says so in one line: "… and unlinked from 3 threat scenarios". | S |
| UI-10.4 | Errors that belong to a field show next to the field, in the inspector, in the error color with an icon. Errors that belong to the whole action show as an error toast that stays until closed. No error opens a dialog. | M |
| UI-10.5 | Confirmation stays only for actions undo cannot reverse (UI-12). | M |

### UI-11 Editors in panels, not over them (S, job 117; after 082)

| ID | Requirement | Pri |
|---|---|---|
| UI-11.1 | The description editor is a multi-line field inside the inspector that grows with its text up to half the panel height, then scrolls. It saves on focus loss and ⌘/Ctrl+Enter, and Escape reverts. The modal description editor is removed. | M |
| UI-11.2 | The attack tree library opens in the right panel slot (082) next to the canvas, so the user can select a gate or step on the canvas while the library is open. The Insert button follows the canvas selection live and says what it will do ("Insert under 'Gain CAN access'"). | M |
| UI-11.3 | The outline editor (text form of the tree) is a mode of the attack tree page that replaces the canvas area, with Apply and Cancel in the page header, not a dialog. | S |
| UI-11.4 | Rename project: click the project name in the header to edit it in place (same rules as UI-08.3). Create baseline: an inline row at the top of the Versions table with a name field. | S |
| UI-11.5 | The keyboard shortcut list moves into the command palette (081) as a "Keyboard shortcuts" view; until 081 lands it may stay a dialog. | C |

### UI-12 Dialog policy (M, every UI job)

A modal dialog is allowed only when one of these is true:

1. The action cannot be undone: delete project, break another user's lock,
   reset a user's password, end other users' sessions, import that replaces
   data.
2. The task is about security and must hold focus: log in, change password,
   set a password.
3. The operating system owns it: open and save file pickers.
4. Information with no work context: About.

Every dialog that stays follows these rules: it names the thing in the title
("Delete project 'ECU Gen3'?"); the main button says the action ("Delete
project", never "Yes" or "OK"); a destructive main button is red and is not the
default for Enter; Escape cancels; focus starts in the first field or on Cancel.

| ID | Requirement | Pri |
|---|---|---|
| UI-12.1 | The rules above are added to [engineering-standards.md](engineering-standards.md) and to the UX review checklist used by `review-ux`. | M |
| UI-12.2 | A test lists the modal overlays in `app.slint` (the `*-open: Rectangle` overlays) against an allow list, so a new dialog fails the build until it is added to the list with a reason. | S |

---

## 4. Every dialog today and what happens to it

| Today | After | Job |
|---|---|---|
| Add asset, damage, control, goal, claim, component, trust boundary (one-line name dialog) | Draft in the inspector | 114 |
| New attack tree (name dialog) | Draft in the inspector; from a threat row, + | 114, 115 |
| Inline "Add a ..." forms for threats, interfaces, data flows | Draft in the inspector | 114 |
| New project | Inline card on the start screen | 114 |
| Rename project | Edit in place in the header | 117 |
| Create baseline | Inline row in the Versions table | 117 |
| Remove (asset, damage, threat, tree, control, ...) confirmation | Immediate, with undo toast | 116 |
| Description editor | Field in the inspector | 117 |
| Outline editor | Mode of the attack tree page | 117 |
| Attack tree library | Right panel slot | 117 (after 082) |
| Keyboard shortcuts | Command palette view | 081, 117 |
| Delete project, break lock, reset or set password, change password | Stay as dialogs (UI-12) | 116 (wording) |
| About | Stays | none |

---

## 5. States every page must handle

Each job above checks its pages in these states, with a screenshot each.

| State | What the user sees |
|---|---|
| Empty | One sentence on what belongs here and one main action ("No assets yet. Assets are what must be protected." + **New asset**). The empty state of 085 uses the draft flow of 114. |
| Draft | The draft row and the draft inspector (UI-08) |
| Read only | No row + buttons; the header button disabled with the reason; inspector fields shown as text, not disabled inputs |
| Error | Field errors inline; action errors as a toast that stays (UI-10.4) |
| Slow server | The optimistic change shows at once; if the confirm takes more than 1 second, a small spinner on the row; the "numbers checked" mark (047) follows as today |
| Japanese | No clipped text at 100 % and 150 % scale; IME works in every draft field |

---

## 6. Order of work

| Order | Job | Why this order |
|---|---|---|
| 1 | 113 platform fidelity | The biggest change in how solid the app feels, with the smallest code change. Independent of the rest. The spike decides the renderer before anything else is polished. |
| 2 | 114 modeless creation | Removes most dialogs. Uses the inspector that exists today. |
| 3 | 116 undo toasts | Small; removes the remaining everyday dialogs; adds the toast that 114 needs for server errors. |
| 4 | 115 data table | Gives the row + and ⋯; prepares 084 and 085. |
| 5 | 082 panel slots, then 117 | The library panel needs the slots. |
| 6 | 081, 083, 084, 085 | As planned; 085 reviews the result as a whole. |

---

## 7. How we check that it worked

- **Task timing**, recorded in each job's UX review, before and after, on macOS
  and Windows: add five assets; add a threat scenario to an asset and rate it;
  remove an asset and undo; insert a library tree under a gate; write a
  description. The goal: no dialog in any of these tasks, and each one done
  with the keyboard alone.
- **Two-person check** after 114 and 116: the owner and one consultant colleague
  do the five tasks on a Mac without help, say out loud where they hesitate,
  and rate each task from 1 (very hard) to 7 (very easy). Anything under 5 goes
  back into the backlog.
- **Screenshot set**: every page, light and dark, English and Japanese, macOS and
  Windows, kept with job 085.

---

## 8. Not in scope

- **A native rewrite (SwiftUI on macOS, WinUI or WPF on Windows).** Rejected in
  D25: every screen and the canvas would be built twice, the Rust core would
  need two bindings, Linux and remote desktops would be lost, and months of work
  would go into no new capability. Reconsider only if a paying customer
  requires a native Mac app after jobs 113 to 117 are done.
- A web UI.
- Theme editor or user-defined colors.
- Inline cell editing inside tables. The inspector is the one place to edit; this
  keeps one pattern and works with long Japanese text.
- Drag and drop between tables (later, if users ask).

---

## 9. Risks

| Risk | What we do |
|---|---|
| Skia adds build time and size and needs a download at build time | The spike measures it (UI-07.2); FemtoVG is the fallback choice; 07.3 guards the offline build |
| A GPU renderer fails on some customer laptops or virtual desktops | Automatic software fallback (07.1) and a setting to force it |
| The headless screenshot tests use the software renderer while users see the GPU one | Keep the tests on software (they check layout, not pixels of anti-aliasing); the spike's screenshots and the per-job manual screenshots cover the GPU look |
| Cupertino style controls are taller or wider and break a tight layout | 085 checks every page; the pseudo-locale test (066) already catches clipping |
| Users miss the confirmation dialog and delete by mistake | Undo toast with ⌘Z in the text; history panel (083) as the second net |
| A draft is lost when the app closes | Accepted: a draft is one name being typed. A non-empty draft is created on window close like on focus loss. |

---

## 10. Questions for the owner

1. Default appearance: follow the system (proposed), or keep the obsidian dark
   theme as the default with System as an option?
2. The shortcut for "new element on this page": ⌘/Ctrl+Shift+N (proposed), or
   another?
3. Is a bigger download (Skia, roughly 10 to 20 MB more, measured in the spike)
   acceptable for crisper text?
