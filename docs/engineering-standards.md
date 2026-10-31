# Engineering Standards

- Status: requirements. They apply to all production code from the first commit.
- Date: 2026-10-15
- Scope: every crate in AegisTARA: model, engine, store, server, desktop client, CLI.

AegisTARA has two goals: a product, and a way to learn software engineering well.
So every rule here has a **why**, and many have a **how to measure**.
The rules use MUST (required), SHOULD (default, deviations need a written reason), and MAY (allowed).

## TLDR

1. **Measure, then optimize.** Every performance claim needs a benchmark or a profile. No guessing.
2. **Budgets are requirements.** Latency, memory, and startup have numbers (section 2). CI fails when a budget breaks.
3. **Pick data structures for the access pattern.** Arenas with integer IDs, not pointer graphs. Contiguous memory for hot data. Deterministic maps where output must be stable.
4. **Know the complexity.** Every algorithm on project data documents its big-O. Interactive paths never scale worse than O(n log n) in project size, and edits cost O(what changed).
5. **Do not waste.** No needless clones, allocations, or copies in hot paths. Reuse buffers. Stream large data.
6. **Correct first, then fast.** Strong types, no panics in library code, property tests, fuzzing.
7. **Write down why.** Every non-trivial choice gets a short decision record with the alternatives and the numbers.

---

## 1. Principles

| # | Principle | Why |
|---|---|---|
| P1 | Correctness before speed | A fast wrong risk score is worse than a slow right one. |
| P2 | Design for efficiency, optimize with evidence | Data layout is hard to change later, so choose it well now. Micro-tuning is easy to add later, so do it only after profiling. |
| P3 | Simple until measured otherwise | The simplest code that meets the budget wins. Complexity must earn its place with numbers. |
| P4 | Deterministic by default | The same input gives the same output, byte for byte. Needed for signed reports, hashes, diffs, and tests. |
| P5 | Costs are visible | Allocation, cloning, locking, and I/O should be easy to spot when reading code. |
| P6 | Every rule is teachable | If we cannot explain why a rule exists, we remove it. |
| P7 | Features earn their complexity | A feature that adds technical complexity without business value is reduced or removed (owner rule, decision D23). A spec names the business value of each moving part. |

---

## 2. Performance budgets

These are requirements. They are checked by benchmarks and tests (section 8) on a reference project.

**Reference projects** (generated, checked into the repo as generators, not as data):

| Name | Size |
|---|---|
| `small` | 13 nodes (the spike tree) |
| `typical` | 10,000 attack graph nodes, 300 assets, 2,000 threats |
| `large` | 100,000 nodes |
| `stress` | 1,000,000 nodes |

**Budgets** (reference machine: a 4-core laptop from 2022; the desktop budgets also hold on the CPU renderer):

| Area | Budget | Project |
|---|---|---|
| Desktop cold start to first frame | ≤ 1.0 s | any |
| Open project from local cache | ≤ 1.0 s | large |
| Interaction latency (click to visible response) | ≤ 50 ms; frame time ≤ 16 ms while panning | large |
| Recompute after one rating change | ≤ 5 ms | large |
| Full recompute from scratch | ≤ 200 ms | large; ≤ 3 s for stress |
| Diff two versions | proportional to the change; ≤ 50 ms for 100 changed nodes | large |
| Client memory (resident) | ≤ 300 MB | large |
| Server memory per open project | ≤ 3x the project's serialized size | any |
| Server API latency | p99 ≤ 100 ms for single-project reads | typical |
| PDF report | ≤ 10 s | typical |
| Stored size | ≤ 150 bytes per node on average after compression | large |

A budget may change only through a decision record that explains why.

---

## 3. Data structures

### Rules

| # | Rule | Why |
|---|---|---|
| D1 | **MUST** store graphs in arenas (`Vec<Node>`) and refer to nodes by typed integer IDs (`NodeIdx(u32)`), not `Rc<RefCell<..>>` or pointers. | Cache-friendly, no reference cycles, cheap to copy, trivial to serialize, safe under the borrow checker. |
| D2 | **MUST** use newtype IDs per entity (`NodeIdx`, `AssetIdx`, `FactorIdx`). | Passing an asset ID where a node ID is expected becomes a compile error. |
| D3 | **MUST** separate a stable public ID (UUIDv7, used in files, APIs, and history) from the internal index (u32, used in memory). Keep one map between them. | Stable IDs survive edits and merges. Internal indices stay small and fast. |
| D4 | **SHOULD** use struct-of-arrays for hot data that is scanned often (for example, factor values of all steps). Keep cold data (names, rationale text) in a separate array. | The engine's inner loop reads only what it needs, so more of it fits in cache. |
| D5 | **MUST** intern repeated strings (labels, catalog IDs). Use `Arc<str>` or an interner, never many owned copies. | Thousands of nodes share a few hundred labels. |
| D6 | **SHOULD** use `SmallVec` (or similar) for lists that are almost always short, such as children of a node. | Avoids one heap allocation per node in the common case. Measure before adopting widely. |
| D7 | **MUST** choose the map by need: `BTreeMap` or `IndexMap` when iteration order reaches output (files, hashes, reports); a fast hasher (`FxHashMap`, `ahash`) for internal maps with trusted keys; the default SipHash for maps keyed by untrusted input. | Determinism (P4), speed, and protection against hash-flooding attacks. |
| D8 | **SHOULD** use bitsets (`fixedbitset`) for visited sets and flags over node indices. | 1 bit per node instead of a hash set entry. |
| D9 | **MUST** use persistent (immutable, structurally shared) collections, such as `imbl`, for snapshots: undo, what-if branches, concurrent readers. | A snapshot costs O(1) instead of copying the project. |
| D10 | **MUST** keep derived data (scores, layout, indexes) separate from source data, and mark it as a cache. | One source of truth. Caches can be dropped and rebuilt. |
| D11 | **SHOULD** prefer `enum` dispatch over `Box<dyn Trait>` in hot paths. Use trait objects at plugin and I/O boundaries. | Static dispatch can be inlined. Dynamic dispatch is fine where flexibility matters more than nanoseconds. |
| D12 | **MUST** make invalid states unrepresentable where cheap: for example, an `enum Node { Gate { .. }, Step { .. } }` instead of one struct with optional fields. | The compiler checks rules that tests might miss. |

### Lesson from our own spike

`spikes/spike-model` was written for speed of building, not for production. It breaks several rules on purpose, and is a good study case:

| Spike code | Cost | Production rule |
|---|---|---|
| `undo_stack.push(self.root.clone())` | Copies the whole tree on every edit: O(n) time and memory per change | D9 and section 4: store inverse operations or persistent snapshots |
| Recursive `find(node, id)` | O(n) search on every lookup | D1 and D3: an ID map gives O(1) |
| `layout()` and `recompute()` rebuild everything | O(n) on every change | A1: incremental recompute, O(affected nodes) |
| `String` IDs and labels cloned everywhere | Many small allocations | D5: interning and integer IDs |

---

## 4. Algorithms

### Rules

| # | Rule | Why |
|---|---|---|
| A1 | **MUST** recompute incrementally: a change reevaluates only the changed nodes and their ancestors (and dependents across modules). Use a dependency graph with memoization (for example `salsa`, or a hand-written dirty-flag scheme). | A rating change in a 100,000-node project touches only a few dozen nodes. |
| A2 | **MUST** evaluate DAGs once per node, in topological order, with memoized results. Shared subtrees must not be evaluated twice. | Naive recursion on a DAG can be exponential. |
| A3 | **MUST** detect cycles before evaluation (Kahn's algorithm or Tarjan's SCC, both O(V + E)) and report them as errors with the cycle path. | Clear errors instead of stack overflows. |
| A4 | **MUST NOT** materialize all attack paths. Use lazy top-k enumeration (priority queue), Pareto fronts, or decision diagrams (BDD/ZDD) for minimal cut sets. | Path counts grow exponentially with tree size. |
| A5 | **MUST** use a spatial index (R-tree, for example `rstar`) for canvas hit testing and culling. | O(log n) per query instead of scanning every node per mouse move. |
| A6 | **MUST** use linear-time tree layout (Buchheim) and incremental layout where possible. | Layout runs often during editing. |
| A7 | **MUST** compute version diffs in time proportional to the change (prolly tree or Merkle comparison). | Change impact analysis on large projects stays instant. |
| A8 | **MUST** document the time and space complexity of every public function that takes project-sized input, in its doc comment. | Makes costs visible (P5) and reviewable. |
| A9 | **MUST NOT** put O(n²) or worse algorithms (in project size) on interactive paths. Batch jobs MAY use them up to the reference size if documented. | Keeps the UI responsive at 100,000 nodes. |
| A10 | **SHOULD** prefer a well-known algorithm with a citation over an invented one. Name it in a comment. | Easier to verify, test, and learn from. |

### Complexity targets

| Operation | Target |
|---|---|
| Rating change, recompute | O(depth x fan-out of changed path) |
| Full evaluation | O(V + E) |
| Hit test, viewport culling | O(log n + k), with k the visible nodes |
| Layout, full | O(n) |
| Undo or redo of one operation | O(size of the operation) |
| Diff of two versions | O(changes x log n) |
| Top-k easiest paths | O(k x path length x log n) after O(V + E) preparation |
| Search across catalogs | O(log n) per term (inverted index) |

---

## 5. Memory and resource use

| # | Rule | Why |
|---|---|---|
| R1 | **MUST NOT** clone in hot paths to satisfy the borrow checker. Borrow, use `Cow`, share with `Arc`, or restructure. | Clones hide allocation cost. Fighting the borrow checker usually means the data layout is wrong. |
| R2 | **SHOULD** preallocate (`with_capacity`) when the size is known, and reuse buffers across frames and recomputes (`clear()` instead of a new `Vec`). | Fewer allocations, less fragmentation. |
| R3 | **MUST** stream large inputs and outputs (imports, exports, reports). Never load a whole multi-gigabyte file into memory. | Memory budget, and protection against hostile files. |
| R4 | **MUST** bound every queue and channel. Producers wait (backpressure) instead of growing memory without limit. | Unbounded queues are memory leaks under load. |
| R5 | **MUST NOT** block inside async code. CPU-heavy work goes to `spawn_blocking` or a `rayon` pool. Async is for I/O. | A blocked async worker stalls every request on it. |
| R6 | **MUST** put a timeout and a size limit on every network request, file read, and plugin call. | Prevents hangs and resource exhaustion. |
| R7 | **MUST** support cancellation of long jobs (reports, Monte Carlo, optimization) and stop work whose result is no longer needed. | Saves CPU, keeps the UI responsive. |
| R8 | **SHOULD** use zero-copy deserialization (`rkyv`) for local caches only, and a stable versioned format for archives and exchange. | Fast loads without locking the long-term format to struct layout. |
| R9 | **MUST** release resources deterministically (RAII). No reliance on process exit for cleanup of files, locks, or connections. | Correct under long-running servers. |
| R10 | **SHOULD** keep binaries lean: enable LTO and `codegen-units = 1` for release, strip symbols in shipped builds, and review dependency count before adding a crate. | Smaller download, faster start, smaller attack surface. |

---

## 6. Concurrency

| # | Rule | Why |
|---|---|---|
| C1 | **MUST** give each open project a single owner (one actor task) that applies operations in order. Others send messages. | No shared mutable state, no lock ordering bugs, natural audit order. |
| C2 | **MUST NOT** hold a lock across an `.await` or across I/O. | Avoids deadlocks and stalls. |
| C3 | **SHOULD** prefer message passing and immutable snapshots over locks. Where locks are needed, keep them short and document the lock order. | Simpler reasoning. |
| C4 | **MUST** keep the engine single-threaded and deterministic for a given input. Parallelism MAY be used for independent work (several projects, Monte Carlo runs, report pages). | Reproducible results (P4). |
| C5 | **MUST** test concurrent code with tools made for it (`loom` for lock-free code, stress tests for actors). | Concurrency bugs rarely show up in normal tests. |

---

## 7. Code quality and safety

| # | Rule | Why |
|---|---|---|
| Q1 | **MUST** pass `cargo fmt --check` and `cargo clippy -- -D warnings` in CI. Selected `clippy::pedantic` lints SHOULD be enabled per crate. | Consistent, idiomatic code. |
| Q2 | **MUST** `#![forbid(unsafe_code)]` in model, engine, method, and lint crates. Elsewhere, every `unsafe` block MUST have a `// SAFETY:` comment and be covered by Miri tests. | Memory safety is a core promise of the product. |
| Q3 | **MUST NOT** `unwrap()` or `expect()` in library code, except where an invariant is proven and commented. Libraries return typed errors (`thiserror`). Applications add context (`anyhow` or similar) at the edge. | No surprise panics in a customer's server. |
| Q4 | **MUST** validate all external input at the boundary, then use validated types inside ("parse, don't validate"). | One place for checks, and types that carry the guarantee. |
| Q5 | **MUST** keep public APIs small and documented, with examples that compile (doctests). | Easier to learn, harder to misuse. |
| Q6 | **SHOULD** keep functions short and single-purpose, and modules organized by domain concept, not by technical layer. | Readable code teaches better. |
| Q7 | **MUST** pass `cargo-deny` (licenses, advisories, bans, sources) and pin the minimum supported Rust version. | Supply chain safety and the license policy (decision D5). |

---

## 8. Testing and measurement

| Level | Tool | Requirement |
|---|---|---|
| Unit tests | built-in `#[test]` | Every public function with logic. |
| Property tests | `proptest` | Engine invariants, for example: adding an easier OR child never makes the parent harder; undo after any operation restores the exact previous state; serialization round-trips. |
| Snapshot tests | `insta` | Reports, display lists, exports. |
| Fuzzing | `cargo-fuzz` | Every parser and importer (pack files, project files, ARXML, ReqIF, SysML). |
| Mutation testing | `cargo-mutants` | Engine and model crates, run weekly. Surviving mutants show weak tests. |
| Benchmarks | `criterion` or `divan` | Every budget in section 2 has a benchmark on the reference projects. CI fails on a regression above 10%. |
| Allocation profiling | `dhat` | Hot paths have allocation-count tests (for example, "recompute after one change allocates at most N times"). |
| CPU profiling | `samply` or `cargo flamegraph` | Required before any optimization pull request. Attach the before and after profile. |
| Tracing | `tracing` spans | Long operations emit spans with timings. Production builds can export them. |
| GUI tests | headless harness (as in `spikes/tools`) | Core interactions are scripted and checked on the CPU renderer. |
| Coverage | `cargo-llvm-cov` | Tracked, not gamed. Engine and model aim for 90% or more line coverage. |

**Optimization workflow (required):**

1. Write or find a benchmark that shows the problem.
2. Profile. Find the actual hot spot.
3. Change one thing.
4. Measure again. Keep the change only if it helps and stays readable.
5. Record the numbers in the pull request.

---

## 9. Learning practices

Because this project is also a school, these practices are part of the definition of done.

| # | Practice | Output |
|---|---|---|
| L1 | **Decision records.** Every non-trivial choice (a data structure, algorithm, crate, or pattern) gets a short entry: context, options, decision, numbers. | `docs/decisions.md` or `docs/adr/NNNN-title.md` |
| L2 | **Benchmarks as lessons.** When two approaches compete (for example `HashMap` vs `BTreeMap`, clone-on-undo vs inverse ops), keep both in the benchmark suite with a comment on what the numbers teach. | `benches/` |
| L3 | **Spike, then production.** Explore fast in `spikes/`, then rewrite properly. Never promote spike code by copy-paste. | Clean production crates |
| L4 | **Review checklist** (below) on every pull request, even when working alone. | Pull request template |
| L5 | **One concept per milestone.** Each milestone names the main engineering concept it teaches (for example: arenas and IDs, incremental computation, actors). | `docs/learning-log.md` |
| L6 | **Read the source.** Before adding a dependency, read its core module and write two lines on how it works. | Dependency notes |

### Pull request checklist

- [ ] Does it meet the budgets? Benchmarks run, numbers attached if the change touches a hot path.
- [ ] Is the big-O documented for new project-sized operations?
- [ ] Are data structures chosen for the access pattern, with the choice explained?
- [ ] No new clones, allocations, or locks in hot paths without a reason.
- [ ] Errors are typed. No new `unwrap` in library code.
- [ ] Tests cover the change: unit, plus property or fuzz where it fits.
- [ ] Output is deterministic where it reaches files, hashes, or reports.
- [ ] Docs and decision records are updated.
- [ ] `fmt`, `clippy`, `cargo-deny`, and tests pass.

---

## 10. Anti-patterns to avoid

| Anti-pattern | Instead |
|---|---|
| `Rc<RefCell<Node>>` object graphs | Arena + typed indices (D1) |
| Cloning a whole structure for undo | Inverse operations or persistent snapshots (D9) |
| Linear search by ID | ID map (D3) |
| Recomputing everything on every change | Incremental recompute (A1) |
| Collecting all paths into a `Vec` | Lazy top-k or decision diagrams (A4) |
| `HashMap` iteration order in output | `BTreeMap` or `IndexMap` (D7) |
| `String` for every label | Interning (D5) |
| Unbounded channels | Bounded channels with backpressure (R4) |
| Blocking file or CPU work in async tasks | `spawn_blocking` or `rayon` (R5) |
| "It feels faster" | A benchmark and a profile (section 8) |
| Premature micro-optimization | Measure first (P2, P3) |

---

## 11. Dialogs and confirmation (UI-12)

Undo is the safety net, so a change that Undo can bring back happens at once,
with a toast that says what happened and offers Undo (UI-10.8, UI-10.9). A
modal dialog is allowed only when one of these is true:

1. The action cannot be undone: delete project, break another user's lock,
   reset or change a password, end other users' sessions, import that replaces
   data.
2. The task is about security and must hold focus: log in, change password,
   set a password.
3. The operating system owns it: open and save file pickers.
4. Information with no work context: About.

A dialog that stays: names the thing in the title ("Delete project 'ECU
Gen3'?"); has a main button that says the action ("Delete project", never "Yes"
or "OK"); makes a destructive main button red and not the default for Enter;
cancels on Escape; starts focus in the first field or on Cancel.

Errors do not open dialogs. A field error shows next to the field (error colour
and icon); an action error is a sticky error toast (UI-10.11).

**How it is enforced:** `crates/aegis-shell/tests/dialogs.rs` lists every modal
overlay and every confirmation question in `app.slint` with a reason. A new one
fails the build until it is added there; adding it needs a reason a reviewer
accepts. `crates/aegis-client/tests/undo_every_op.rs` applies and undoes every
`Op` kind, which is what makes removing a confirmation safe (UI-10.6).
