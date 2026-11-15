#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
CARGO_HOME_DIR=${CARGO_HOME:-"$HOME/.cargo"}
SEPARATOR=$(printf '\037')
FLAGS=

append_flag() {
  if [ -n "$FLAGS" ]; then
    FLAGS="$FLAGS$SEPARATOR$1"
  else
    FLAGS=$1
  fi
}

# Cargo prefers CARGO_ENCODED_RUSTFLAGS even when it is set to an empty value.
if [ "${CARGO_ENCODED_RUSTFLAGS+x}" = x ]; then
  FLAGS=$CARGO_ENCODED_RUSTFLAGS
elif [ -n "${RUSTFLAGS:-}" ]; then
  set -f
  for flag in $RUSTFLAGS; do
    append_flag "$flag"
  done
  set +f
fi

append_flag "--remap-path-prefix=$CARGO_HOME_DIR=/cargo"
append_flag "--remap-path-prefix=$ROOT=/build"

export CARGO_ENCODED_RUSTFLAGS="$FLAGS"
cd "$ROOT"
exec "$ROOT/node_modules/.bin/tauri" build --target universal-apple-darwin
