#!/bin/bash
# run_test.sh - Executa teste de latência

set -e

INSTANCE_TYPE=${1:-"unknown"}
OPTIMIZE_NETWORK=${2:-"false"}
MIN_TRADES=${3:-"100000"}

echo "Teste de Latência - Binance Trades"
echo "Instance: $INSTANCE_TYPE | Trades: $MIN_TRADES"
echo ""

# Otimizações de rede (opcional)
if [ "$OPTIMIZE_NETWORK" = "true" ]; then
    echo "Aplicando otimizações de rede..."
    bash optimize_network.sh
    echo ""
fi

# Compila
cargo build --release

# Executa
CSV_FILE="latency_${INSTANCE_TYPE}_$(date +%s).csv"
CSV_FILE="$CSV_FILE" \
MACHINE_ID="$INSTANCE_TYPE" \
MIN_TRADES="$MIN_TRADES" \
REALTIME=0 \
./target/release/binance-trades

echo ""
echo "Arquivo: $CSV_FILE"

