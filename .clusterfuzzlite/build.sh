#!/bin/bash
set -euo pipefail

cd $SRC/clickup-commands
cargo fuzz build -O parsers
cp fuzz/target/x86_64-unknown-linux-gnu/release/parsers $OUT/
