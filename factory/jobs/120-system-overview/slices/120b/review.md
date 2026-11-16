VERDICT: PASS
## For the owner
What changed: a new `combined.rs` lists every data flow whose two ends sit in different systems ("ADAS → Gateway"), with how many threat scenarios cover it.
The item definition's data flow rows and the flow detail now carry that boundary text, the threat count and a "no threat" flag.
Each Systems table row now says how many boundary flows the system has and with whom ("Gateway 1", "2 systems 2").
Look at:
- crates/aegis-client/src/pages.rs:686 (`flow_boundary_info`): `no_threat` is true for a flow that crosses a trust boundary OR a system boundary and has 0 threats. The prompt asks for this, but the flag now also applies to trust-boundary flows inside one system. Check that this is what the screen should show.
- crates/aegis-client/src/combined.rs:73 (`threats_per_component`): a flow counts as covered when either end component has a bound asset with a threat. This is the same rule the lint uses, so a flow touching a well-covered ECU never shows as uncovered.
- crates/aegis-client/src/systems.rs:69: per-system counting. A flow counts once for each end's system, so Gateway shows 2 for the fixture.
Risk: low. The change is data only: no UI, no new operation, no dependency. All inputs come from the in-memory project. Lookups are bounds-checked (`get`/`get_mut`), so no panic can be reached. Ordering is deterministic (arena order, then BTreeMap/BTreeSet). Five new tests cover the fixture the prompt specifies. Each would fail without the change: the new fields would not exist, or the counts would be wrong.
View: git -C factory/worktrees/120-system-overview diff 550d275140e2cc35ad85e932e12cf089fdc53f9e..HEAD
## Findings
- None blocking.
- Minor, no change required: the "A → B" text is built twice, in `combined::boundary_flows` and `pages::flow_boundary_info`. A later slice could share one helper. Two cases have no test: a flow from a component to itself (the threat count should count that end once) and `no_threat` set by a trust-boundary crossing alone. Neither was a "Done when" item.
## Lesson
none
NEXT: human
