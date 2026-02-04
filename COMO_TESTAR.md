# 🧪 Guia de Teste - Binance Latency Test

Guia completo de como testar o sistema de medição de latência da Binance.

## 📋 Opções de Teste

### 1️⃣ Teste Rápido (Recomendado para começar)

```bash
./test_quick.sh
```

**Características:**
- ✅ Coleta ~1000 trades em ~30 segundos
- ✅ Mostra estatísticas em tempo real
- ✅ Salva em CSV automaticamente
- ✅ Compila automaticamente se necessário
- ✅ Ideal para validar que está funcionando

---

### 2️⃣ Teste Manual - Modo Tempo Real

```bash
# 1. Compilar (se ainda não compilou)
cargo build --release

# 2. Executar (sem salvar, apenas visualizar)
MACHINE_ID=test-local ./target/release/binance-trades
```

**Características:**
- ✅ Mostra estatísticas atualizando a cada 1 segundo
- ✅ Para parar: Pressione `Ctrl+C`
- ✅ Ideal para ver funcionando rapidamente
- ✅ Não salva dados (apenas visualização)

---

### 3️⃣ Teste com CSV (1000 trades)

```bash
CSV_FILE=teste.csv MACHINE_ID=test-local MIN_TRADES=1000 \
./target/release/binance-trades
```

**Características:**
- ✅ Coleta 1000 trades e salva em CSV
- ✅ Mostra estatísticas em tempo real
- ✅ Ideal para análise posterior
- ✅ Para automaticamente após coletar 1000 trades

---

### 4️⃣ Teste Completo (100k trades - Produção)

```bash
./run_test.sh m8a.xlarge false 100000
```

**Características:**
- ✅ Coleta 100k trades (leva vários minutos)
- ✅ Sem display (mais rápido)
- ✅ Salva em CSV com timestamp
- ✅ Ideal para testes sérios em AWS

**Parâmetros:**
- `m8a.xlarge`: Identificador da máquina
- `false`: Não aplicar otimizações de rede
- `100000`: Número mínimo de trades

---

### 5️⃣ Teste Tempo Real com CSV

```bash
./run_realtime.sh m8a.xlarge latency.csv
```

**Características:**
- ✅ Mostra estatísticas em tempo real
- ✅ Salva em CSV simultaneamente
- ✅ Ideal para monitorar enquanto coleta
- ✅ Para com `Ctrl+C`

---

## 📊 O que Você Verá Quando Estiver Funcionando

```
Conectando a wss://stream.binance.com:9443/ws/btcusdt@trade...
Machine ID: test-local
Modo tempo real: ATIVADO (atualiza a cada 1s)

Conectado! Coletando dados em tempo real...

[test-local] Trades: 1523 | Lat: Avg=142.3ms p50=140.1ms p95=180.5ms p99=220.0ms | Jitter=15.2ms | TPS=45.3 | Gaps=0 OOO=0
```

**Legenda:**
- `Trades`: Total de trades coletados
- `Lat`: Latência (Avg=média, p50=mediana, p95=p95, p99=p99)
- `Jitter`: Desvio padrão (variação de latência)
- `TPS`: Trades por segundo (throughput)
- `Gaps`: Trades perdidos (gaps detectados)
- `OOO`: Trades fora de ordem (out-of-order)

Esta linha atualiza a cada 1 segundo.

---

## ✅ Verificações Rápidas

### Sinais de que está funcionando corretamente:

- ✅ **Trades incrementando**: Número aumenta continuamente
- ✅ **Latência realista**: 50-300ms (depende da região)
- ✅ **TPS > 0**: Throughput ativo
- ✅ **Gaps = 0**: Idealmente nenhum trade perdido
- ✅ **Estatísticas finais**: Ao parar (`Ctrl+C`), mostra resumo completo

### Exemplo de Estatísticas Finais:

```
=== Estatísticas Finais ===
Machine ID: test-local
Total de trades: 1000

--- Latência ---
  Média: 142.30ms
  Mediana (p50): 140.10ms
  p95: 180.50ms
  p99: 220.00ms
  Mínima: 138.00ms
  Máxima: 250.00ms
  Jitter (std): 15.20ms

--- Validações ---
  Trades perdidos (gaps): 0
  Trades fora de ordem: 0

--- Performance ---
  Throughput: 45.30 trades/segundo
```

---

## 🚀 Comece Agora

Para um teste rápido de validação:

```bash
./test_quick.sh
```

Isso vai:
1. Compilar o projeto (se necessário)
2. Conectar ao WebSocket da Binance
3. Coletar ~1000 trades
4. Mostrar estatísticas em tempo real
5. Salvar em CSV automaticamente

---

## 🔧 Variáveis de Ambiente

Você pode customizar o comportamento usando variáveis de ambiente:

| Variável | Descrição | Padrão |
|----------|-----------|--------|
| `MACHINE_ID` | Identificador da máquina | `unknown` |
| `CSV_FILE` | Arquivo CSV para salvar dados | (não salva) |
| `MIN_TRADES` | Número mínimo de trades (0 = infinito) | `0` |
| `REALTIME` | Mostrar estatísticas em tempo real (`1` ou `0`) | `1` |
| `STATS_SAMPLES` | Tamanho da amostra para percentis | `10000` |

### Exemplos:

```bash
# Teste sem display, salvando em CSV
REALTIME=0 CSV_FILE=latency.csv MACHINE_ID=m8a.xlarge MIN_TRADES=100000 \
./target/release/binance-trades

# Teste com amostra maior para percentis
STATS_SAMPLES=20000 MACHINE_ID=test-local \
./target/release/binance-trades
```

---

## ⚠️ Problemas Comuns

### "Erro ao conectar"
- **Causa**: Problema de conexão com internet ou Binance
- **Solução**: Verifique sua conexão e tente novamente

### Latência negativa
- **Causa**: Problema de sincronização de relógio (raro)
- **Solução**: Verifique se o relógio do sistema está correto

### Gaps > 0
- **Causa**: Perda de mensagens (pode ser rede instável)
- **Solução**: Normal em redes instáveis, mas idealmente deve ser 0

### TPS = 0
- **Causa**: Não está recebendo trades
- **Solução**: Verifique se compilou com `--release` e se a conexão está ativa

---

## 📝 Próximos Passos

Após validar que está funcionando:

1. **Teste em AWS**: Execute em instâncias AWS para comparar latência
2. **Compare instâncias**: Execute simultaneamente em múltiplas máquinas
3. **Analise resultados**: Compare os CSVs gerados para encontrar a melhor configuração
4. **Otimize rede**: Use `optimize_network.sh` antes dos testes para melhor performance

---

## 📚 Mais Informações

Para mais detalhes, consulte:
- `README.md` - Documentação completa do projeto
- `src/main.rs` - Código fonte com comentários detalhados


