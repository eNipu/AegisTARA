GATE: FAIL
Slice: 141b  Base: 62036ff59e787a2535ea1125fcd759e628e634e8

- command failed: scripts/ci.sh crates/aegis-model crates/aegis-client crates/aegis-workbench

Diff: 698 counted lines, 45 files
FAIL scripts/ci.sh crates/aegis-model crates/aegis-client crates/aegis-workbench
```
              b: 0,
              c: 446,
              bits: 29984143381,
              list: [],
          },
          Seed {
              kind: 0,
              a: 0,
              b: 7016,
              c: 3,
              bits: 1276293227708864261,
              list: [],
          },
          Seed {
              kind: 84,
              a: 0,
              b: 0,
              c: 26,
              bits: 0,
              list: [],
          },
          Seed {
              kind: 66,
              a: 1,
              b: 0,
              c: 14723,
              bits: 0,
              list: [],
          },
          Seed {
              kind: 42,
              a: 0,
              b: 0,
              c: 391,
              bits: 0,
              list: [],
          },
      ], cut = 10499498006071497777
      	successes: 3
      	local rejects: 0
      	global rejects: 0
      
      
      
      failures:
          replay_first_methodology
          replay_second_methodology
          snapshot_round_trip_first_methodology
          snapshot_round_trip_second_methodology
      
      test result: FAILED. 4 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.99s
      
      error: test failed, to rerun pass `--test store_properties`
ok    aegis-client fmt
ok    aegis-client clippy
ok    aegis-client test
ok    aegis-workbench fmt
ok    aegis-workbench clippy
ok    aegis-workbench test
1 step(s) failed in 3 crate(s): aegis-model/test
```
ok   cargo test --locked -p aegis-model --test shared_steps --test store_limits --test ops_properties
ok   cargo test --locked -p aegis-client --test undo_every_op
