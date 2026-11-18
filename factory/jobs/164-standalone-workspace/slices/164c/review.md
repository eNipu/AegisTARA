VERDICT: PASS
## For the owner
What changed: new `aegis_workbench::convert` with `is_legacy` and `convert_legacy`. `convert_legacy` reads an old job-107 folder through `FolderStore` and `Loaded::rebuild`, then writes workspace files beside the old files. It deletes nothing.
It refuses a folder made with another methodology key or version. The refusal message now cuts each value read from the folder to 64 characters and adds `…` when it cuts (rework 1).
Four tests: a snapshot plus a rename op converts to the same project with the old files byte-identical; another version is refused; a 10 000-character key gives a short message; a folder without a manifest is not legacy.
Look at: crates/aegis-workbench/src/convert.rs:49-56 (the runtime is dropped before rebuild and convert_beside, which is what frees `.aegis.lock`); crates/aegis-workbench/src/convert.rs:72-80 and 90-99 (the refusal message and the `shown` cut).
Risk: low. The earlier finding about unbounded folder values in the refusal message is fixed. `a_long_methodology_key_is_cut_in_the_message` fails without `shown` (the message would be over 10 000 bytes). The equality test fails if the ops are dropped, because the rename makes the project differ from the snapshot. The version test fails without the guard, because the snapshot itself loads with the real methodology. Other error texts pass through server and client errors unchanged; this slice adds no new input in them.
View: git -C factory/worktrees/164-standalone-workspace diff 69b8b21f223c75c0bac5205b69ec66b646c6b031..HEAD
## Findings
none
## Lesson
none
NEXT: human
