# Teste de Latência - Binance Trades

Sistema para testar e comparar latência de trades da Binance em diferentes configurações de instâncias AWS.

## 🎯 Como o Teste Funciona

### 1. Conexão WebSocket
- Conecta ao stream de trades da Binance: `wss://stream.binance.com:9443/ws/btcusdt@trade`
- Recebe trades de BTC/USDT em tempo real

### 2. Medição de Latência
Para cada trade recebido:
1. **Captura timestamp de recebimento** (`recv_ts`) - momento exato que a mensagem chega na máquina
2. **Extrai timestamp do trade** (`ts`) - momento que o trade aconteceu (campo `T` do JSON)
3. **Calcula latência**: `latency_ms = recv_ts - ts`

### 3. Processamento
- **Zero parsing JSON**: Apenas extrai os campos `t` (trade_id) e `T` (timestamp) por busca de string
- **Estatísticas lock-free**: Usa atomics para atualização sem locks
- **Display em tempo real**: Mostra estatísticas atualizadas a cada 1 segundo

### 4. Coleta de Dados
- Salva em CSV: `trade_id,ts,recv_ts,latency_ms,machine_id`
- Permite merge posterior por `trade_id` para comparar mesmos trades entre máquinas

## 🧪 Como Testar

### Teste Rápido (30 segundos)
```bash
# Teste rápido que coleta ~1000 trades
./test_quick.sh
```

### Teste Manual (Tempo Real)
```bash
# 1. Compilar
cargo build --release

# 2. Executar em modo tempo real (sem salvar)
MACHINE_ID=test-local ./target/release/binance-trades

# Você verá estatísticas atualizando a cada 1 segundo:
# [test-local] Trades: 1523 | Lat: Avg=142.3ms p50=140.1ms p95=180.5ms p99=220.0ms | Jitter=15.2ms | TPS=45.3 | Gaps=0 OOO=0
```

### Teste Completo (100k trades)
```bash
# Coleta 100k trades e salva em CSV
./run_test.sh m8a.xlarge false 100000
```

## 🚀 Uso

### Compilar
```bash
cargo build --release
```

### Executar

**Modo Tempo Real (recomendado):**
```bash
# Apenas visualização
MACHINE_ID=m8a.xlarge ./target/release/binance-trades

# Com CSV (salva enquanto mostra)
CSV_FILE=latency_m8a.csv MACHINE_ID=m8a.xlarge ./target/release/binance-trades
```

**Modo Coleta (sem display):**
```bash
REALTIME=0 CSV_FILE=latency_m8a.csv MACHINE_ID=m8a.xlarge MIN_TRADES=100000 ./target/release/binance-trades
```

### Exemplo de Saída (Tempo Real)
```
[m8a.xlarge] Trades: 1523 | Lat: Avg=142.3ms p50=140.1ms p95=180.5ms p99=220.0ms | Jitter=15.2ms | TPS=45.3 | Gaps=0 OOO=0
```
Esta linha atualiza a cada 1 segundo.

**Legenda:**
- `Lat`: Latência (Avg=média, p50=mediana, p95=p95, p99=p99)
- `Jitter`: Desvio padrão (variação de latência)
- `TPS`: Trades por segundo (throughput)
- `Gaps`: Trades perdidos (gaps detectados)
- `OOO`: Trades fora de ordem (out-of-order)

## 📊 Variáveis de Ambiente

- `MACHINE_ID`: Identificador da máquina (obrigatório para comparação)
- `CSV_FILE`: Arquivo CSV para salvar dados (opcional)
- `MIN_TRADES`: Número mínimo de trades (0 = infinito, padrão: 0)
- `REALTIME`: Mostrar estatísticas em tempo real (1 = sim, 0 = não, padrão: 1)
- `STATS_SAMPLES`: Tamanho da amostra para cálculo de percentis (padrão: 10000)

## 🧪 Teste em Múltiplas Instâncias AWS

### Passo 1: Executar em cada instância

**Instância 1 (m8a.xlarge):**
```bash
CSV_FILE=latency_m8a.csv MACHINE_ID=m8a.xlarge ./target/release/binance-trades
```

**Instância 2 (z1d.xlarge):**
```bash
CSV_FILE=latency_z1d.csv MACHINE_ID=z1d.xlarge ./target/release/binance-trades
```

**Instância 3 (c8i.xlarge):**
```bash
CSV_FILE=latency_c8i.csv MACHINE_ID=c8i.xlarge ./target/release/binance-trades
```

### Passo 2: Comparar resultados

Baixe os CSVs e compare manualmente ou use ferramentas de análise:
- Mesmos `trade_id` = mesmo trade
- Compare `latency_ms` entre máquinas
- Menor latência = melhor configuração

## 📁 Formato do CSV

```
trade_id,ts,recv_ts,latency_ms,machine_id
5827967018,1769693418802,1769693418944,142.00,m8a.xlarge
```

- `trade_id`: ID único do trade (para merge)
- `ts`: Timestamp do trade (da Binance)
- `recv_ts`: Timestamp de recebimento na máquina
- `latency_ms`: Latência calculada (recv_ts - ts)
- `machine_id`: Identificador da máquina

## ⚙️ Otimizações de Rede (Opcional)

Para aplicar otimizações de rede antes do teste:
```bash
bash optimize_network.sh
```

## 📈 Métricas e Validações

### Estatísticas de Latência
- **Avg**: Latência média
- **p50 (Mediana)**: 50% dos trades têm latência ≤ este valor
- **p95**: 95% dos trades têm latência ≤ este valor
- **p99**: 99% dos trades têm latência ≤ este valor
- **Min**: Latência mínima observada
- **Max**: Latência máxima observada
- **Jitter**: Desvio padrão (variação de latência) - menor é melhor

### Validações de Integridade
- **Gaps**: Número de trades perdidos (detecta quando `trade_id` pula números)
- **Out-of-Order (OOO)**: Número de trades recebidos fora de ordem
  - Trades devem chegar em ordem crescente de `trade_id`
  - Se `trade_id` atual < `trade_id` anterior = fora de ordem

### Métricas de Performance
- **TPS (Trades Per Second)**: Throughput - quantos trades por segundo estão sendo processados
- **Total de trades**: Contador total de trades coletados

## 🔍 Como o Teste Funciona (Detalhado)

### Fluxo de Processamento

```
1. WebSocket recebe mensagem JSON da Binance
   ↓
2. Captura timestamp de recebimento (recv_ts) - IMEDIATAMENTE
   ↓
3. Extrai trade_id e timestamp do trade (ts) - busca de string, sem parsing JSON
   ↓
4. Calcula latência: latency_ms = recv_ts - ts
   ↓
5. Atualiza estatísticas (lock-free com atomics)
   ↓
6. Salva no CSV (se habilitado)
   ↓
7. Mostra estatísticas em tempo real (a cada 1s)
```

### Por que é rápido?
- **Zero parsing JSON**: Apenas busca strings `"t":` e `"T":` no texto (não deserializa JSON completo)
- **Lock-free**: Usa atomics para estatísticas, sem locks ou spawn por trade
- **Mínimo overhead**: Apenas extrai timestamp e calcula diferença

### O que a latência mede?
A latência `recv_ts - ts` inclui:
- ✅ Tempo de rede (Binance → sua máquina)
- ✅ Overhead do WebSocket/TCP
- ✅ Processamento mínimo (extração de timestamp)

**NÃO inclui**: Parsing JSON completo, logging, I/O de arquivo (se assíncrono)

### Exemplo de Mensagem JSON
```json
{"e":"trade","E":1769693418944,"s":"BTCUSDT","t":5827967018,"p":"88120.26","q":"0.00008","T":1769693418802,"m":false}
```
- Campo `t`: trade_id (5827967018)
- Campo `T`: timestamp do trade (1769693418802)
- O código busca esses campos diretamente, sem deserializar o JSON completo

## 🔍 Interpretando os Resultados

### O que procurar em uma boa configuração?
1. **Latência baixa**: p50, p95, p99 próximos da média
2. **Jitter baixo**: Variação consistente (std dev < 10ms ideal)
3. **Zero gaps**: Nenhum trade perdido
4. **Zero OOO**: Trades chegam em ordem
5. **TPS alto**: Capacidade de processar muitos trades/segundo

### Comparando Instâncias
- **Melhor latência**: Menor p50, p95, p99
- **Mais consistente**: Menor jitter
- **Mais confiável**: Zero gaps e zero OOO
- **Mais performático**: Maior TPS

## ✅ Verificando se Está Funcionando

### Sinais de que está funcionando:
1. ✅ **Conexão estabelecida**: Mensagem "Conectado!" aparece
2. ✅ **Trades incrementando**: Número de trades aumenta continuamente
3. ✅ **Latência realista**: Valores entre 50-300ms (depende da região)
4. ✅ **TPS > 0**: Throughput mostra trades por segundo
5. ✅ **Gaps = 0**: Idealmente nenhum trade perdido
6. ✅ **Estatísticas finais**: Ao parar (Ctrl+C), mostra resumo completo

### Problemas comuns:
- ❌ **"Erro ao conectar"**: Verifique conexão com internet
- ❌ **Latência negativa**: Problema de sincronização de relógio (raro)
- ❌ **Gaps > 0**: Perda de mensagens (pode ser rede instável)
- ❌ **TPS muito baixo**: Verifique se está usando `--release`

## ⚠️ Importante

- **Sempre use `--release`** para performance real
- **Região próxima** (ap-southeast-1) terá menor latência
- Execute simultaneamente em múltiplas máquinas para comparar os mesmos trades
- Colete pelo menos 100k trades para estatísticas confiáveis
- **Gaps > 0** indica perda de mensagens (problema de rede/WebSocket)
- **OOO > 0** indica que trades chegam fora de ordem (pode ser normal em alta carga)
