#!/bin/bash
# run_test.sh - Executa teste de latência (funciona local e AWS)

set -e

INSTANCE_TYPE=${1:-"unknown"}
OPTIMIZE_NETWORK=${2:-"false"}
COUNT=${3:-"100000"}
SYMBOL=${4:-"btcusdt"}

echo "=== Teste de Latência - Binance Trades ==="
echo "Instance: $INSTANCE_TYPE"
echo "Symbol:   $SYMBOL"
echo "Trades:   $COUNT"
echo ""

# Otimizações de rede (opcional)
if [ "$OPTIMIZE_NETWORK" = "true" ]; then
    echo "Aplicando otimizações de rede..."
    sudo ./optimize_network.sh
    echo ""
fi

# Compila se necessário
if [ ! -f "./target/release/binance-trades" ]; then
    echo "Compilando..."
    cargo build --release
    echo ""
fi

# Executa
CSV_FILE="latency_${INSTANCE_TYPE}_$(date +%s).csv"
CSV_FILE="$CSV_FILE" \
MACHINE_ID="$INSTANCE_TYPE" \
./target/release/binance-trades "$SYMBOL" "$COUNT"

echo ""
echo "✅ Arquivo gerado: $CSV_FILE"

