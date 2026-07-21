#!/usr/bin/env bash

set -e
set -x

cargo clippy --all-targets --all-features
cargo test
cargo test -F "serde"
cargo test -F "serde-nontransparent"
cargo run --example serde --features="serde"

exit 0
