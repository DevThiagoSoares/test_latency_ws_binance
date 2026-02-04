# Teste de Latência - Binance WebSocket

Sistema para medir latência de trades da Binance em diferentes configurações de servidor.

## 🎯 Como Funciona

1. Conecta ao WebSocket da Binance: `wss://stream.binance.com:9443/ws/btcusdt@trade`
2. Para cada trade recebido:
   - Captura timestamp de recebimento (`recv_ts`)
   - Extrai timestamp do trade (`ts`) do JSON
   - Calcula latência: `latency_ms = recv_ts - ts`
3. Salva dados brutos em CSV: `trade_id,ts,recv_ts,latency_ms,machine_id`

**Otimizações:**
- I/O em thread separada (fora do hot path)
- Zero parsing JSON completo (apenas extrai campos necessários)
- Hot path mínimo: receber → extrair → calcular → enviar

## 🚀 Como Usar

### Compilar

```bash
cargo build --release
```

### Teste Local (Validação)

```bash
# Teste rápido: ~1000 trades
CSV_FILE=test.csv MACHINE_ID=local MIN_TRADES=1000 ./target/release/binance-trades

# Teste com display em tempo real
MACHINE_ID=local REALTIME=1 ./target/release/binance-trades
```

### Teste Completo (Local ou AWS)

```bash
# Usando script (recomendado)
./run_test.sh m8a.xlarge false 100000

# Ou manualmente
CSV_FILE=latency_m8a_$(date +%s).csv \
MACHINE_ID=m8a.xlarge \
MIN_TRADES=100000 \
REALTIME=0 \
./target/release/binance-trades
```

**Parâmetros do script:**
- `m8a.xlarge`: Identificador da máquina (use o tipo da instância AWS ou nome customizado)
- `false`: Não aplicar otimizações de rede (use `true` para aplicar)
- `100000`: Número mínimo de trades

### Executar em Múltiplas Instâncias (AWS)

**Instância 1:**
```bash
./run_test.sh m8a.xlarge false 100000
```

**Instância 2:**
```bash
./run_test.sh z1d.xlarge false 100000
```

**Instância 3:**
```bash
./run_test.sh c8i.xlarge false 100000
```

**Importante:** Execute simultaneamente para comparar os mesmos trades.

### Otimizações de Rede (AWS - Opcional)

```bash
# Aplicar otimizações antes do teste
sudo ./optimize_network.sh

# Depois execute o teste normalmente
./run_test.sh m8a.xlarge false 100000
```

## 📊 Variáveis de Ambiente

| Variável | Descrição | Padrão |
|----------|-----------|--------|
| `MACHINE_ID` | Identificador da máquina | `unknown` |
| `CSV_FILE` | Arquivo CSV para salvar | (não salva) |
| `MIN_TRADES` | Número mínimo de trades (0 = infinito) | `0` |
| `REALTIME` | Mostrar contador em tempo real (`1` ou `0`) | `1` |

## 📁 Formato do CSV

```csv
trade_id,ts,recv_ts,latency_ms,machine_id
5827967018,1769693418802,1769693418944,142.00,m8a.xlarge
5827967019,1769693418900,1769693419045,145.00,m8a.xlarge
```

- `trade_id`: ID único do trade (para JOIN entre máquinas)
- `ts`: Timestamp do trade (da Binance)
- `recv_ts`: Timestamp de recebimento na máquina
- `latency_ms`: Latência calculada em milissegundos
- `machine_id`: Identificador da máquina

## 📈 Análise dos Resultados

**Importante:** Calcule estatísticas **após fazer JOIN** dos CSVs por `trade_id`.

### Exemplo com Python/Pandas

```python
import pandas as pd

# Carregar CSVs
df_m8a = pd.read_csv('latency_m8a_123456.csv')
df_z1d = pd.read_csv('latency_z1d_123456.csv')

# JOIN por trade_id
df_joined = df_m8a.merge(
    df_z1d[['trade_id', 'latency_ms']], 
    on='trade_id', 
    suffixes=('_m8a', '_z1d')
)

# Estatísticas
print("M8A - Média:", df_joined['latency_ms_m8a'].mean())
print("M8A - Mediana:", df_joined['latency_ms_m8a'].median())
print("M8A - p95:", df_joined['latency_ms_m8a'].quantile(0.95))
print("M8A - p99:", df_joined['latency_ms_m8a'].quantile(0.99))

print("Z1D - Média:", df_joined['latency_ms_z1d'].mean())
print("Z1D - Mediana:", df_joined['latency_ms_z1d'].median())
```

## 🛠️ Setup na AWS

### 1. Instalar Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 2. Clonar Repositório

```bash
git clone https://github.com/DevThiagoSoares/test_latency_ws_binance.git
cd test_latency_ws_binance
```

### 3. Compilar

```bash
cargo build --release
```

### 4. Executar Teste

```bash
./run_test.sh m8a.xlarge false 100000
```

### 5. Baixar Resultados

```bash
# No seu computador local
scp -i sua-chave.pem ec2-user@IP-INSTANCIA:~/test-infra/latency_*.csv ./
```

## ⚠️ Troubleshooting

**Erro ao conectar:**
- Verifique conexão com internet
- Verifique Security Group (porta 9443)

**CSV não é criado:**
- Verifique se `CSV_FILE` está configurado
- Verifique permissões de escrita

**Teste para antes de completar:**
- Use `screen` ou `tmux` para sessões persistentes
- Execute com `nohup` em background

## 📝 Notas

- Estatísticas devem ser calculadas **após JOIN** por `trade_id`
- Execute testes simultaneamente para comparar mesmos trades
- Colete pelo menos 100k trades para análise estatística válida
- Região AWS próxima aos servidores da Binance = menor latência
