# ADR 0006: Explain-why traces for attack tree evaluation

- Status: proposed (job 006-explain-why)
- Date: 2026-10-20
- Crate: `crates/aegis-model` (`src/eval.rs`)
- Rules: P1, P3, P4, D4, D10, A2, A4, A8, A9, R1, R2, Q3, Q5, L1 in
  `docs/engineering-standards.md`
- Builds on: `0005-tree-evaluation.md` (attribute domains, `evaluate`)

## Context

Goal G6 in `docs/brainstorm.md` is "every number explains itself"
(section 8.2): each derived value carries a derivation trace, the inputs,
and which path dominated ("Feasibility High comes from path P3"). This is
the engine side of the "Why?" button and the report appendix.

ADR 0005 evaluates a tree with a left fold per gate over a pluggable
`AttributeDomain`. A generic fold sees only values, so it cannot tell which
child produced a gate's value. Guessing by value equality is wrong for sums
(a SAND of 0 and 5 equals its second child, but both contributed) and for
types whose `Ord` ignores part of the value. Only the domain knows, so the
domain has to say it.

The owner confirmed the spec's open questions on 2026-10-20 (`rework.md`):
provided `*_kept` methods with a `Both` default, always record the trace,
name only the earlier child on ties, record `Combined` without tracking
which children contributed, and no human-readable text in this job.

## Options

### How the domain reports which child decided (Open question 1)

| Option | Good | Bad |
|---|---|---|
| **A. Provided methods `or_kept`, `and_kept`, `seq_and_kept` returning `(V, Kept)`, default `(op(acc, next), Kept::Both)`** | No breaking change: existing domains compile and stay correct, because `Both` ("all inputs contributed") is always truthful; one code path in `evaluate`; the default calls the plain method exactly once, so the 005 call-count tests stay valid | Two methods per gate kind that must agree; mitigated by having the built-ins implement the plain method through the `_kept` one |
| B. Separate `TracedDomain: AttributeDomain` trait | Plain domains are untouched | Two code paths in `evaluate` (or specialization, which Rust does not have) |
| C. Guess by value equality | No domain change | Wrong for sums and for partial `Ord` (see Context) |

**Chosen: A.**

### When to record (Open question 2)

| Option | Good | Bad |
|---|---|---|
| **Always, in `evaluate`** | One API; every evaluation can explain itself (the product promise); two equal evaluations have equal traces (P4) | 8 bytes per node and a few percent of time, also for callers that never ask |
| Separate `evaluate_traced` | Zero cost when unused | Two entry points to keep in sync; callers must know in advance that a "Why?" will be asked |

**Chosen: always.** The full-recompute budget had 99% headroom (ADR 0005:
1.9 ms of 200 ms at `large`), and the measured cost below is about 0.07 ms.

## Decision

```rust
pub enum Kept { Acc, Next, Both }

// provided methods on AttributeDomain (same for and_kept, seq_and_kept):
fn or_kept(&self, acc: Self::Value, next: &Self::Value) -> (Self::Value, Kept) {
    (self.or(acc, next), Kept::Both)
}

pub enum Derivation {
    Leaf,
    Selected { rule: GateKind, child: NodeIdx },
    Combined { rule: GateKind },
}
```

Contract for `*_kept`: the value MUST equal the plain method's; `Acc` only
if the result is `acc` unchanged, `Next` only if it is `next`'s value alone,
otherwise `Both`. `seq_and_kept` defaults to `seq_and` (not to `and_kept`),
because a domain may override `seq_and` alone.

Built-in domains override the `_kept` methods and implement the plain ones
through them, so one comparison serves both and they cannot disagree:

- `MaxMin<T>`: OR (max) and AND/SAND (min) answer `Next` when `next`
  strictly replaced `acc`, else `Acc`.
- `MinTime`: OR (min) and AND (max) the same way; SAND (saturating sum)
  always `Both`.

`kept_matches_plain` checks the value and the tag for all 65,536 `u8`
pairs of `MaxMin<u8>` and 64 `u64` pairs of `MinTime`, including
`u64::MAX`.

### State machine

A gate folds its children `c1, ..., ck` in stored order:

1. Start `Selected(c1)`: the first child's value is cloned.
2. For each next child `ci`: `Acc` keeps the state; `Next` sets
   `Selected(ci)`; `Both` sets `Combined`.
3. A later `Next` after `Combined` returns to `Selected(ci)`, because the
   value was replaced in full.

A one-child gate is `Selected(c1)`. A step is `Leaf`. `kept_state_machine`
drives a scripted domain through all transitions for all three gate kinds.

**Ties:** the built-ins keep `acc` on a tie, so the trace names the earlier
child in stored order (a linked child counts at its stored position). Tied
later children are not listed; `explain` shows every input with its value,
so a reader can see them.

**`Combined` granularity:** after a `Both`, the record says only "several
children contributed". It does not track which prefix of children did when
a later `Acc` follows. Precise enough for v1 (confirmed).

### Storage and layout

`Evaluation<V>` gains `derivations: Vec<Derivation>` in arena order, filled
in the same loop as the values (`with_capacity(tree.len())`, R2). It is a
separate array next to `values` (struct-of-arrays, D4), so trace reads do
not touch the values. It is derived data like the values (D10), and the
existing derives (`Clone`, `Debug`, `PartialEq`, `Eq`) cover it.

**Measured size:** `size_of::<Derivation>() == 8` on aarch64 (`NodeIdx` is a
`u32`, `GateKind` a 1-byte enum, plus the tag; alignment 4), asserted to be
at most 8 by `derivation_size`. At `large` that is 0.8 MB, at `stress`
8 MB, about 0.3% and 2.7% of the 300 MB client memory budget. `Kept` is 1
byte.

### Read API (all on `Evaluation<V>`)

| Function | Cost | Notes |
|---|---|---|
| `derivation(idx) -> Option<Derivation>` | O(1) | `None` outside the evaluation |
| `explain(&tree, idx) -> Option<Explanation>` | O(1), no allocation | Borrows the tree for the child list (R1). `None` if `idx` is outside, the tree has a different node count, or the node kind or gate kind does not match the record |
| `Explanation::inputs()` | O(k) for k children, no allocation | Children in fold order with their values |
| `why_path(idx)` | O(depth) time, O(1) space, lazy, no recursion | Yields `idx`, then follows `Selected` children; stops after a `Leaf` or `Combined` node; at most `len()` items; does not need the tree |

`Explanation` also offers `node()`, `value()`, `derivation()`, `rule()`,
and `decisive()`. It carries no text: wording, labels, and localisation
belong to the UI and report jobs.

The node-count check in `explain` goes slightly beyond the spec's kind
checks: it is O(1) and rejects any tree edited by `add_child`. It cannot
catch an edit that keeps the count and the kinds (a new `link`); the rule
stays "recompute after every edit", and the incremental recompute (A1)
will own that later.

### Shared subtrees

ADR 0005 evaluates a shared node once and reuses its value for every
parent. For selection domains (`MaxMin`, `MinTime` OR and AND) this is
exact, and so is the trace: a shared step has one value and one
derivation, so a `why_path` that reaches it through any parent continues
the same way (`why_path_follows_decisive` follows a path through a linked
child). For sum-like rules the trace says `Combined`, and the ADR 0005
limitation (a shared step counted once per path) applies to the value, not
to the trace.

## Complexity

`evaluate`: still O(V + E) time, one pass, same errors, same counts (V
calls of `leaf` for steps, E - G domain calls). Extra work O(1) per fold
step (one match on `Kept`, one 8-byte store per node). Extra space O(V):
order, values, and derivations. `why_path` O(depth), `explain` O(1) plus
O(k) for `inputs`. Nothing is O(V^2) (A9). `trace_deterministic_and_deep`
walks a 100,000-deep chain on the default test thread (100,001 items).

## Numbers

Machine: Apple M2 Max (12 cores, 32 GB), macOS 26.6.2, `rustc 1.99.1
(58cf8f9c3 2026-10-13) (Homebrew)`, `cargo bench --bench eval` (release
profile), divan 0.1.21, 20 samples, one iteration per sample. Same workload
as ADR 0005 (`benches/eval.rs`, `MaxMin<u8>`, unchanged). "Before" is the
base commit `a4c3e21` (two runs: one in the job worktree before any change,
one from a `git archive` copy of the base after the change, interleaved
with the "after" runs). "After" is this job (two runs).

| size | before median (runs) | after median (runs) | overhead | after, share of budget | overhead, share of budget |
|---|---:|---:|---:|---:|---:|
| large (100,000 nodes) | 1.951 ms (1.955, 1.946) | 2.022 ms (2.013, 2.031) | +0.071 ms, +3.6% | 1.0% of 200 ms | 0.04% |
| stress (1,000,000 nodes) | 20.28 ms (20.14, 20.41) | 21.16 ms (21.14, 21.18) | +0.88 ms, +4.3% | 0.7% of 3 s | 0.03% |

`topological_order_only` is unchanged (1.25 ms / 12.9 ms in all runs), so
the overhead is in the fold and the result allocation, as expected.

### What the numbers teach

1. **Always-on was the right call.** The trace costs about 4% of a pass that
   itself uses 1% of its budget: 0.07 ms at `large`. An opt-in
   `evaluate_traced` would have doubled the API to save time nobody can
   see.
2. **The cost is the extra array, not the bookkeeping.** One more
   8-byte-per-node `Vec` to fill and write (0.8 MB at `large`, 8 MB at
   `stress`) matches the slightly larger relative overhead at `stress`,
   where the arrays no longer fit in cache. If it ever matters, reusing the
   buffers across recomputes (R2) is the lever.

## Consequences

- New public API: `Kept`, `Derivation`, `Explanation`, the provided methods
  `or_kept`, `and_kept`, `seq_and_kept`, and `Evaluation::derivation`,
  `explain`, `why_path`. Non-breaking for existing domains.
- Domains written outside this crate get `Combined` for every gate with two
  or more children until they override the `_kept` methods; that is
  truthful but less useful, and is documented on the trait.
- Out of scope, for later jobs: human-readable text, the "Why?" button, the
  report appendix, traces for formulas, risk matrices and attack potential,
  incremental recompute, top-k paths (A4), and serialization of traces.

## Reproduce

From `crates/aegis-model/`:

```
cargo test --test explain             # derivations, ties, state machine, explain, why_path, naive agreement
cargo test --lib kept_matches_plain   # built-in *_kept equal the plain methods
cargo test --lib derivation_size      # Derivation <= 8 bytes
cargo bench --bench eval              # before and after medians
```
