#!/bin/bash
# test_quick.sh - Teste rápido (30 segundos, ~1000 trades)

set -e

echo "🧪 Teste Rápido - Binance Latency Test"
echo "======================================"
echo ""

# Compila se necessário
if [ ! -f "./target/release/binance-trades" ]; then
    echo "📦 Compilando..."
    cargo build --release
    echo ""
fi

# Executa teste rápido (coleta ~1000 trades, leva ~30 segundos)
echo "▶️  Executando teste rápido (coleta ~1000 trades)..."
echo ""

CSV_FILE="test_quick_$(date +%s).csv" \
MACHINE_ID="test-local" \
MIN_TRADES=1000 \
REALTIME=1 \
./target/release/binance-trades

echo ""
echo "✅ Teste concluído!"
echo ""
echo "📊 Verifique o arquivo CSV gerado para ver os dados coletados:"
ls -lh test_quick_*.csv 2>/dev/null | tail -1 || echo "   (nenhum arquivo encontrado)"

