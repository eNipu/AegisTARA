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
| 1 | LicenseRef-Proprietary (this spike) |
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
