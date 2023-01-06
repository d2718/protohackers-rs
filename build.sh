#!/bin/sh

set -xeuo pipefail

TARGET=x86_64-unknown-linux-musl
BINARY=target/$TARGET/release/$1

cargo build --target $TARGET --bin $1 --release

strip $BINARY
mv $BINARY ./