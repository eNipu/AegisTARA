#!/bin/sh
# Build and run the aegis-model fuzz targets (ADR 0009) for a bounded time.
#
# Usage: crates/aegis-model/fuzz/run.sh [SECONDS] [TARGET...]
#   SECONDS  run time per target (default 60)
#   TARGET   snapshot, op_log (default: both)
#
# Works on the stable toolchain and needs no cargo-fuzz: it builds each
# target with the same libFuzzer coverage flags cargo-fuzz passes (sancov,
# --cfg fuzzing, debug assertions, overflow checks), without a sanitizer.
# `--target` keeps the flags off build scripts. With nightly and cargo-fuzz
# installed, `cargo +nightly fuzz run <target> corpus/<target> seeds/<target>`
# from this directory does the same with AddressSanitizer.
#
# New inputs go to corpus/<target>/, crashes to artifacts/<target>/ (both
# git-ignored); the committed seeds/<target>/ are read only. Exits non-zero
# on the first crash.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
secs=${1:-60}
[ "$#" -gt 0 ] && shift
[ "$#" -eq 0 ] && set -- snapshot op_log

host=$(rustc -vV | sed -n 's/^host: //p')
RUSTFLAGS="--cfg fuzzing -Cpasses=sancov-module \
-Cllvm-args=-sanitizer-coverage-level=4 \
-Cllvm-args=-sanitizer-coverage-inline-8bit-counters \
-Cllvm-args=-sanitizer-coverage-pc-table \
-Cllvm-args=-sanitizer-coverage-trace-compares \
-Cdebug-assertions -Coverflow-checks" \
    cargo build --manifest-path "$here/Cargo.toml" --release --locked --bins --target "$host"

for target in "$@"; do
    mkdir -p "$here/corpus/$target" "$here/artifacts/$target"
    echo "fuzzing $target for ${secs}s"
    "$here/target/$host/release/$target" \
        -max_total_time="$secs" \
        -artifact_prefix="$here/artifacts/$target/" \
        "$here/corpus/$target" "$here/seeds/$target"
done
