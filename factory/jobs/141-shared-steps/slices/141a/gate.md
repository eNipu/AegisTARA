GATE: FAIL
Slice: 141a  Base: e1fbce3ae912db99efab539434175fdf18752f94

- command failed: scripts/ci.sh crates/aegis-model

Diff: 560 counted lines, 13 files
FAIL scripts/ci.sh crates/aegis-model
```
              ],
          },
          Seed {
              kind: 22,
              a: 62034,
              b: 43348,
              c: 41704,
              bits: 14337568469188546914,
              list: [
                  59642,
                  4913,
                  478,
                  14861,
              ],
          },
          Seed {
              kind: 84,
              a: 49131,
              b: 30791,
              c: 51261,
              bits: 10619397140480277316,
              list: [
                  48102,
              ],
          },
          Seed {
              kind: 34,
              a: 32660,
              b: 39197,
              c: 9591,
              bits: 4288742990983580649,
              list: [],
          },
          Seed {
              kind: 33,
              a: 49826,
              b: 65410,
              c: 50396,
              bits: 4811383556855215736,
              list: [
                  36078,
              ],
          },
      ], cut = 9262621545211544479
      	successes: 5
      	local rejects: 0
      	global rejects: 0
      
      
      
      failures:
          replay_first_methodology
          replay_second_methodology
          snapshot_round_trip_first_methodology
          snapshot_round_trip_second_methodology
      
      test result: FAILED. 4 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.32s
      
      error: test failed, to rerun pass `--test store_properties`
1 step(s) failed in 1 crate(s): aegis-model/test
```
ok   cargo test --locked -p aegis-model --test shared_steps --test ops_properties
