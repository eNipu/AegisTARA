# GPUI shell spike (job 181)

Throwaway spike for decision D37: one window built with GPUI and gpui-component,
with a left dock (project tree), a centre tab area (`Attack tree` placeholder and
an `Item definition` code editor) and a right dock (inspector). Production crates
never use it (decision D8).

Run it with:

```sh
cargo run --release --manifest-path spikes/gpui-shell/Cargo.toml
```

Pinned versions: `gpui-pre 0.3.12` (renamed to `gpui`), `gpui-pre-platform 0.3.12`
(renamed to `gpui_platform`, features `font-kit` and `runtime_shaders`),
`gpui-component 0.9.1`.

## Build and run

macOS:

```sh
cargo build --release --locked --manifest-path spikes/gpui-shell/Cargo.toml
cargo run --release --locked --manifest-path spikes/gpui-shell/Cargo.toml -- --nodes 100000
```

Windows (PowerShell):

```powershell
cargo build --release --locked --manifest-path spikes\gpui-shell\Cargo.toml
cargo run --release --locked --manifest-path spikes\gpui-shell\Cargo.toml -- --nodes 100000
```

Options: `--nodes N` (1 to 1,000,000, default 13), `--measure`, `--frames F` (1 to 10,000, default 300).

## Measure

`--measure` centres the view on the tree, pans it 8 px per frame for `F` frames and prints
`FIRST_FRAME_MS`, `FRAME_INTERVAL_MS p50 p95 max`, `CANVAS_MS p50 p95 max`, `ITEMS`, `NODES`
and `MEASURE_DONE`, then quits after 3 seconds. The scripts run it and add resident memory
and binary size:

```sh
sh spikes/gpui-shell/measure.sh 100000 300          # macOS, Linux
```

```powershell
powershell -ExecutionPolicy Bypass -File spikes\gpui-shell\measure.ps1   # Windows
```

Both build nothing; build the release binary first. Numbers go in `RESULTS.md`.

## Windows and RDP for the owner

1. Install rustup (https://rustup.rs) and the Visual Studio Build Tools with the
   "Desktop development with C++" workload and a Windows SDK.
2. Clone the repository and open PowerShell in it.
3. `cargo build --release --locked --manifest-path spikes\gpui-shell\Cargo.toml`
4. `powershell -ExecutionPolicy Bypass -File spikes\gpui-shell\measure.ps1`
5. Run step 4 once at the laptop and once from the Mac through an RDP client (Microsoft
   "Windows App"), inside the remote session.
6. Run the app by hand too (`cargo run --release --manifest-path spikes\gpui-shell\Cargo.toml -- --nodes 100000`)
   and go through the checklist.
7. Paste the printed lines and tick the checklist in `RESULTS.md`.

## Licenses

Checked 2026-11-11 on the locked dependency set: 760 packages (including this spike).

Command:

```sh
cargo metadata --format-version 1 --manifest-path spikes/gpui-shell/Cargo.toml \
  | python3 -c 'import json,sys,collections;p=json.load(sys.stdin)["packages"];c=collections.Counter(x["license"] or "(none)" for x in p);print(len(p));[print(f"{n}\t{l}") for l,n in c.most_common()]'
```

| Count | License |
|------:|---------|
| 367 | MIT OR Apache-2.0 |
| 148 | MIT |
| 61 | Apache-2.0 OR MIT |
| 32 | Apache-2.0 |
| 29 | MIT/Apache-2.0 |
| 22 | Unicode-3.0 |
| 20 | Zlib OR Apache-2.0 OR MIT |
| 9 | BSD-3-Clause |
| 8 | Unlicense OR MIT |
| 8 | MIT OR Apache-2.0 OR Zlib |
| 5 | ISC |
| 5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| 4 | BSD-2-Clause |
| 4 | Apache-2.0/MIT |
| 4 | Apache-2.0 OR ISC OR MIT |
| 3 | MPL-2.0 |
| 3 | Zlib |
| 2 | 0BSD |
| 2 | MIT / Apache-2.0 |
| 2 | MIT OR Zlib OR Apache-2.0 |
| 2 | BSD-3-Clause OR Apache-2.0 |
| 2 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| 2 | Unlicense/MIT |
| 2 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| 2 | BSD-2-Clause OR Apache-2.0 OR MIT |
| 1 | 0BSD OR MIT OR Apache-2.0 |
| 1 | PolyForm-Noncommercial-1.0.0 (this spike) |
| 1 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| 1 | (Apache-2.0 OR MIT) AND BSD-3-Clause |
| 1 | Apache-2.0 / MIT |
| 1 | CC0-1.0 OR Apache-2.0 |
| 1 | (MIT OR Apache-2.0) AND NCSA |
| 1 | BSD-2-Clause OR MIT OR Apache-2.0 |
| 1 | CC0-1.0 |
| 1 | Apache-2.0 AND ISC |
| 1 | Apache-2.0 OR BSL-1.0 |
| 1 | Apache-2.0 OR GPL-2.0-only |

No GPL-only entry. Only `r-efi` (two versions, `MIT OR Apache-2.0 OR LGPL-2.1-or-later`)
and `self_cell` (`Apache-2.0 OR GPL-2.0-only`) mention GPL or LGPL, and both offer a
permissive alternative. `gpui-pre`, `gpui-pre-platform`, `gpui-component`, `gpui-base`
and the other `gpui-*` crates are Apache-2.0 (`gpui-pre-reqwest` is MIT OR Apache-2.0).
