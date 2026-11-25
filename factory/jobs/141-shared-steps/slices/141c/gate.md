GATE: FAIL
Slice: 141c  Base: ca621b595c90b12f9ad08f19c55f6311e986cc25

- command failed: scripts/ci.sh crates/aegis-model

Diff: 717 counted lines, 9 files
FAIL scripts/ci.sh crates/aegis-model
```
              bits: 0,
              list: [],
          },
          Seed {
              kind: 82,
              a: 0,
              b: 1,
              c: 866,
              bits: 0,
              list: [],
          },
          Seed {
              kind: 22,
              a: 251,
              b: 41,
              c: 388,
              bits: 0,
              list: [],
          },
          Seed {
              kind: 22,
              a: 1,
              b: 0,
              c: 51940,
              bits: 0,
              list: [],
          },
          Seed {
              kind: 30,
              a: 745,
              b: 0,
              c: 3,
              bits: 0,
              list: [],
          },
          Seed {
              kind: 28,
              a: 7905,
              b: 0,
              c: 0,
              bits: 0,
              list: [],
          },
      ], cut = 378605640269231034
      	successes: 3
      	local rejects: 0
      	global rejects: 0
      
      
      
      failures:
          replay_first_methodology
          replay_second_methodology
          snapshot_round_trip_first_methodology
          snapshot_round_trip_second_methodology
      
      test result: FAILED. 4 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.20s
      
      error: test failed, to rerun pass `--test store_properties`
1 step(s) failed in 1 crate(s): aegis-model/test
```
ok   cargo test --locked -p aegis-model --test shared_steps --test ops_properties
