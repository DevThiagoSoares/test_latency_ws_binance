#!/bin/bash
# run_realtime.sh - Executa em modo tempo real

set -e

INSTANCE_TYPE=${1:-"unknown"}
CSV_FILE=${2:-""}

# Compila se necessário
[ ! -f "./target/release/binance-trades" ] && cargo build --release

# Executa
if [ -n "$CSV_FILE" ]; then
    CSV_FILE="$CSV_FILE" MACHINE_ID="$INSTANCE_TYPE" ./target/release/binance-trades
else
    MACHINE_ID="$INSTANCE_TYPE" ./target/release/binance-trades
fi

