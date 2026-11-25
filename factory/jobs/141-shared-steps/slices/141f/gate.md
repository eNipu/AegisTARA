GATE: PASS
Slice: 141f  Base: fc50d6b49fff2e2c10bb59fd7019b93f585b94e1


Diff: 421 counted lines, 28 files
ok   scripts/ci.sh
ok   cargo test --locked -p aegis-client --test shared_steps --test table_conventions --test table_features --test explorer --test undo_every_op
ok   cargo test --locked -p aegis-shell --test nav_groups --test pages --test i18n
ok   cargo test --locked -p aegis-workbench --test table_view
