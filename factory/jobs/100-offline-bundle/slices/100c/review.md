VERDICT: PASS
## For the owner
What changed: The compose file now names the server image (`AEGIS_SERVER_IMAGE`, default `aegistara-server:local`), so a loaded image starts without a build.
`docs/deployment.md` has an offline install section (check, server, desktop, updates), and the release checklist says how to build and publish the bundle.
ADR 0067 records the bundle format and why it was chosen.
Look at: deploy/compose.yaml:29-32 (the one real behavior change; the default keeps `up --build` working as before); docs/deployment.md:34-80 (the verify commands a customer will copy); docs/adr/0067-offline-bundle.md:7 (the "R3 (bounded parsing)" label does not match engineering-standards R3, which is "stream large inputs").
Risk: none for code (docs and deploy only). Compose config check passes per gate. The docs match the facts in the prompt, and nothing is described that the scripts do not do. The ADR's cargo-fuzz wording ("caps and bundle.rs tests bound the parser") is a reasonable reason the builder had to supply.
View: git -C factory/worktrees/100-offline-bundle diff 67ce712230b70a53acd7dcbf7eacb71c7a3d37c7..HEAD
## Findings
- docs/adr/0067-offline-bundle.md:7 — Rules label "R3 (bounded parsing)" does not match the standards file — minor, copied from the prompt; the owner may want to reword it. Not a defect in this slice.
## Lesson
[planner] Quote rule labels from docs/engineering-standards.md exactly when writing prompts; the R3 label in the 100c prompt did not match the file.
NEXT: human
