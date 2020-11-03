#!/usr/bin/env bash

set -e

# RUSTSEC-2020-0016 - https://github.com/SergioBenitez/Rocket/issues/1440 & https://github.com/notify-rs/notify/issues/248
# RUSTSEC-2020-0053 - https://github.com/rusoto/rusoto/pull/1846
# RUSTSEC-2020-0056 - https://github.com/time-rs/time/issues/248
cargo audit --deny-warnings --ignore RUSTSEC-2020-0016 --ignore RUSTSEC-2020-0053 --ignore RUSTSEC-2020-0056

cargo fmt -- --check

cargo clippy --all-targets --all-features -- -D warnings
