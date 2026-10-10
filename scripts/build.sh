#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --release --locked
