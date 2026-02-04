//! Teste de Latência - Binance WebSocket Trades
//!
//! Este programa conecta ao WebSocket da Binance, recebe trades de BTC/USDT em tempo real,
//! e mede a latência entre o momento que o trade aconteceu e quando foi recebido.
//!
//! Uso:
//!   MACHINE_ID=m8a.xlarge ./target/release/binance-trades
//!   CSV_FILE=latency.csv MACHINE_ID=m8a.xlarge MIN_TRADES=100000 ./target/release/binance-trades

use futures_util::StreamExt;
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;
use tokio_tungstenite::{connect_async, tungstenite::Message};

// ============================================================================
// Estrutura de Estatísticas
// ============================================================================

/// Armazena estatísticas de latência e validações de integridade dos trades.
///
/// Usa operações atômicas (lock-free) para atualizações rápidas e thread-safe.
/// Mantém uma amostra recente de latências para cálculo de percentis e jitter.
struct LatencyStats {
    /// Contador total de trades processados
    count: AtomicU64,
    
    /// Soma total de latências em microsegundos (para cálculo da média)
    total_latency: AtomicU64,
    
    /// Latência mínima observada (em microsegundos)
    min: AtomicU64,
    
    /// Latência máxima observada (em microsegundos)
    max: AtomicU64,
    
    /// Amostra recente de latências para cálculo de percentis e jitter
    /// Mantém apenas as últimas N amostras (configurável)
    recent_latencies: Mutex<VecDeque<f64>>,
    
    /// Tamanho máximo da amostra recente
    max_samples: usize,
    
    /// ID do último trade processado (para detectar gaps e ordem)
    last_trade_id: AtomicU64,
    
    /// Número de trades perdidos (gaps) detectados
    gaps_detected: AtomicU64,
    
    /// Número de trades recebidos fora de ordem
    out_of_order: AtomicU64,
    
    /// Timestamp de início da coleta (para cálculo de throughput)
    start_time: SystemTime,
}

impl LatencyStats {
    /// Cria uma nova estrutura de estatísticas.
    ///
    /// # Argumentos
    /// * `max_samples` - Tamanho máximo da amostra para cálculo de percentis
    fn new(max_samples: usize) -> Self {
        Self {
            count: AtomicU64::new(0),
            total_latency: AtomicU64::new(0),
            min: AtomicU64::new(u64::MAX),
            max: AtomicU64::new(0),
            recent_latencies: Mutex::new(VecDeque::with_capacity(max_samples)),
            max_samples,
            last_trade_id: AtomicU64::new(0),
            gaps_detected: AtomicU64::new(0),
            out_of_order: AtomicU64::new(0),
            start_time: SystemTime::now(),
        }
    }

    /// Atualiza as estatísticas com um novo trade.
    ///
    /// # Argumentos
    /// * `trade_id` - ID único do trade (para validação de ordem e gaps)
    /// * `latency_ms` - Latência do trade em milissegundos
    ///
    /// # Funcionalidades
    /// - Atualiza contador e soma de latências (lock-free)
    /// - Atualiza min/max usando compare-and-swap (lock-free)
    /// - Detecta trades perdidos (gaps) comparando trade_ids consecutivos
    /// - Detecta trades fora de ordem
    /// - Mantém amostra recente para cálculo de percentis
    fn update(&self, trade_id: u64, latency_ms: f64) {
        // Converte latência para microsegundos para precisão
        let latency_us = (latency_ms * 1000.0) as u64;
        
        // Atualiza contador e soma (lock-free)
        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_latency.fetch_add(latency_us, Ordering::Relaxed);
        
        // Atualiza mínimo usando compare-and-swap (lock-free)
        loop {
            let current = self.min.load(Ordering::Relaxed);
            if latency_us >= current {
                break; // Não é menor que o atual
            }
            // Tenta atualizar apenas se o valor ainda for o mesmo
            if self.min.compare_exchange(current, latency_us, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                break; // Atualizado com sucesso
            }
            // Se falhou, tenta novamente (outro thread pode ter atualizado)
        }
        
        // Atualiza máximo usando compare-and-swap (lock-free)
        loop {
            let current = self.max.load(Ordering::Relaxed);
            if latency_us <= current {
                break; // Não é maior que o atual
            }
            if self.max.compare_exchange(current, latency_us, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                break; // Atualizado com sucesso
            }
        }

        // Validação de ordem e detecção de gaps
        let last_id = self.last_trade_id.load(Ordering::Relaxed);
        if last_id > 0 {
            if trade_id < last_id {
                // Trade recebido fora de ordem (trade_id menor que o anterior)
                self.out_of_order.fetch_add(1, Ordering::Relaxed);
            } else if trade_id > last_id + 1 {
                // Gap detectado: pulou um ou mais trade_ids (trades perdidos)
                let gap = trade_id - last_id - 1;
                self.gaps_detected.fetch_add(gap, Ordering::Relaxed);
            }
        }
        self.last_trade_id.store(trade_id, Ordering::Relaxed);

        // Mantém amostra recente para cálculo de percentis e jitter
        let mut latencies = self.recent_latencies.lock().unwrap();
        latencies.push_back(latency_ms);
        // Remove amostras antigas se exceder o limite
        if latencies.len() > self.max_samples {
            latencies.pop_front();
        }
    }

    /// Retorna todas as estatísticas calculadas.
    ///
    /// # Retorno
    /// Tupla com: (count, avg, min, max, p50, p95, p99, jitter, gaps, out_of_order, throughput)
    /// - count: Total de trades
    /// - avg: Latência média em ms
    /// - min: Latência mínima em ms
    /// - max: Latência máxima em ms
    /// - p50: Percentil 50 (mediana) em ms
    /// - p95: Percentil 95 em ms
    /// - p99: Percentil 99 em ms
    /// - jitter: Desvio padrão (variação) em ms
    /// - gaps: Número de trades perdidos
    /// - out_of_order: Número de trades fora de ordem
    /// - throughput: Trades por segundo
    fn get(&self) -> (u64, f64, f64, f64, f64, f64, f64, f64, u64, u64, f64) {
        let count = self.count.load(Ordering::Relaxed);
        if count == 0 {
            return (0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0, 0, 0.0);
        }
        
        // Calcula média, min e max
        let total = self.total_latency.load(Ordering::Relaxed) as f64 / 1000.0;
        let avg = total / count as f64;
        let min = self.min.load(Ordering::Relaxed) as f64 / 1000.0;
        let max = self.max.load(Ordering::Relaxed) as f64 / 1000.0;

        // Calcula percentis e jitter da amostra recente
        let latencies = self.recent_latencies.lock().unwrap();
        let mut sorted: Vec<f64> = latencies.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let (p50, p95, p99, jitter) = if sorted.is_empty() {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            // Calcula índices para percentis
            let p50_idx = (sorted.len() as f64 * 0.50) as usize;
            let p95_idx = (sorted.len() as f64 * 0.95) as usize;
            let p99_idx = ((sorted.len() as f64 * 0.99) as usize).min(sorted.len() - 1);
            
            let p50 = sorted[p50_idx];
            let p95 = sorted[p95_idx];
            let p99 = sorted[p99_idx];
            
            // Jitter = desvio padrão (mede variação de latência)
            let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
            let variance = sorted.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / sorted.len() as f64;
            let jitter = variance.sqrt();
            
            (p50, p95, p99, jitter)
        };

        // Calcula throughput (trades por segundo)
        let elapsed = self.start_time.elapsed().unwrap().as_secs_f64();
        let throughput = if elapsed > 0.0 { count as f64 / elapsed } else { 0.0 };

        let gaps = self.gaps_detected.load(Ordering::Relaxed);
        let out_of_order = self.out_of_order.load(Ordering::Relaxed);

        (count, avg, min, max, p50, p95, p99, jitter, gaps, out_of_order, throughput)
    }
}

// ============================================================================
// Extração de Dados do JSON
// ============================================================================

/// Extrai trade_id e timestamp do JSON sem fazer parsing completo.
///
/// Esta função é otimizada para performance: em vez de deserializar o JSON completo
/// (que seria lento), ela busca diretamente os campos "t" (trade_id) e "T" (timestamp)
/// fazendo busca de string em bytes.
///
/// # Argumentos
/// * `text` - String JSON da mensagem do WebSocket
///
/// # Retorno
/// `Some((trade_id, timestamp))` se ambos campos foram encontrados, `None` caso contrário
///
/// # Exemplo de JSON
/// ```json
/// {"e":"trade","E":1769693418944,"s":"BTCUSDT","t":5827967018,"p":"88120.26","q":"0.00008","T":1769693418802,"m":false}
/// ```
/// - Campo `t`: trade_id (5827967018)
/// - Campo `T`: timestamp do trade em milissegundos (1769693418802)
fn extract_trade_data(text: &str) -> Option<(u64, u64)> {
    let bytes = text.as_bytes();
    let mut trade_id = None;
    let mut trade_time = None;
    
    // Busca o campo "t":<número> (trade_id)
    for i in 0..bytes.len().saturating_sub(20) {
        if bytes.get(i..i+4)? == b"\"t\":" {
            let mut j = i + 4;
            // Pula espaços após ":"
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            
            // Lê o número
            let mut num = 0u64;
            let start = j;
            
            while j < bytes.len() {
                match bytes[j] {
                    b @ b'0'..=b'9' => {
                        num = num * 10 + (b - b'0') as u64;
                        j += 1;
                    }
                    b',' | b'}' => break, // Fim do número
                    _ => break,
                }
            }
            
            if j > start && num > 0 {
                trade_id = Some(num);
                break;
            }
        }
    }
    
    // Busca o campo "T":<número> (timestamp)
    for i in 0..bytes.len().saturating_sub(20) {
        if bytes.get(i..i+4)? == b"\"T\":" {
            let mut j = i + 4;
            // Pula espaços após ":"
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            
            // Lê o número
            let mut num = 0u64;
            let start = j;
            
            while j < bytes.len() {
                match bytes[j] {
                    b @ b'0'..=b'9' => {
                        num = num * 10 + (b - b'0') as u64;
                        j += 1;
                    }
                    b',' | b'}' => break, // Fim do número
                    _ => break,
                }
            }
            
            // Valida que é um timestamp válido (deve ser > 1000000000000 = ano 2001)
            if j > start && num > 1000000000000 {
                trade_time = Some(num);
                break;
            }
        }
    }
    
    // Retorna ambos se encontrados
    if let (Some(id), Some(ts)) = (trade_id, trade_time) {
        Some((id, ts))
    } else {
        None
    }
}

// ============================================================================
// Função Principal
// ============================================================================

#[tokio::main]
async fn main() {
    // ========================================================================
    // Configuração via Variáveis de Ambiente
    // ========================================================================
    
    let csv_file = std::env::var("CSV_FILE").ok();
    let machine_id = std::env::var("MACHINE_ID")
        .or_else(|_| std::env::var("AWS_REGION"))
        .unwrap_or_else(|_| "unknown".to_string());
    let min_trades: u64 = std::env::var("MIN_TRADES")
        .unwrap_or_else(|_| "0".to_string())
        .parse()
        .unwrap_or(0);
    let show_realtime = std::env::var("REALTIME").unwrap_or_else(|_| "1".to_string()) == "1";
    let max_samples: usize = std::env::var("STATS_SAMPLES")
        .unwrap_or_else(|_| "10000".to_string())
        .parse()
        .unwrap_or(10000);

    // ========================================================================
    // Inicialização de Estatísticas
    // ========================================================================
    
    let stats = std::sync::Arc::new(LatencyStats::new(max_samples));
    let stats_clone = stats.clone(); // Clone para a task de display

    // ========================================================================
    // Configuração de CSV (se habilitado)
    // ========================================================================
    
    let mut csv_writer: Option<std::fs::File> = if let Some(ref file) = csv_file {
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(file)
            .expect(&format!("Erro ao criar CSV: {}", file));
        // Escreve cabeçalho do CSV
        writeln!(f, "trade_id,ts,recv_ts,latency_ms,machine_id").unwrap();
        Some(f)
    } else {
        None
    };

    // ========================================================================
    // Task de Display em Tempo Real
    // ========================================================================
    
    let machine_id_display = machine_id.clone();
    if show_realtime {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            loop {
                interval.tick().await;
                let (count, avg, _min, _max, p50, p95, p99, jitter, gaps, out_of_order, throughput) = stats_clone.get();
                if count > 0 {
                    // Limpa linha anterior e mostra estatísticas atualizadas
                    print!("\r\x1b[K"); // ANSI: volta ao início da linha e limpa
                    print!("[{}] Trades: {} | Lat: Avg={:.1}ms p50={:.1}ms p95={:.1}ms p99={:.1}ms | Jitter={:.1}ms | TPS={:.1} | Gaps={} OOO={}", 
                        machine_id_display, count, avg, p50, p95, p99, jitter, throughput, gaps, out_of_order);
                    io::stdout().flush().unwrap();
                }
            }
        });
    }

    // ========================================================================
    // Conexão WebSocket
    // ========================================================================
    
    let url = "wss://stream.binance.com:9443/ws/btcusdt@trade";
    eprintln!("Conectando a {}...", url);
    eprintln!("Machine ID: {}", machine_id);
    if show_realtime {
        eprintln!("Modo tempo real: ATIVADO (atualiza a cada 1s)\n");
    }

    let (ws_stream, _) = connect_async(url).await.expect("Erro ao conectar");
    if show_realtime {
        eprintln!("\x1b[2J\x1b[H"); // Limpa tela
        eprintln!("Conectado! Coletando dados em tempo real...\n");
    } else {
        eprintln!("Conectado! Coletando dados...\n");
    }

    let (_write, mut read) = ws_stream.split();

    // ========================================================================
    // Loop Principal: Processamento de Mensagens
    // ========================================================================
    
    while let Some(msg) = read.next().await {
        if let Ok(Message::Text(text)) = msg {
            // PASSO 1: Captura timestamp de recebimento IMEDIATAMENTE
            // Isso é crítico para medir latência com precisão máxima
            let recv_ts = SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            // PASSO 2: Extrai trade_id e timestamp do trade (sem parsing JSON completo)
            if let Some((trade_id, ts)) = extract_trade_data(&text) {
                // PASSO 3: Calcula latência = quando recebemos - quando trade aconteceu
                let latency_ms = recv_ts as f64 - ts as f64;
                
                // PASSO 4: Atualiza estatísticas (lock-free, muito rápido)
                // Inclui validações: ordem, gaps, percentis, jitter
                stats.update(trade_id, latency_ms);

                // PASSO 5: Salva no CSV se habilitado
                if let Some(ref mut file) = csv_writer {
                    writeln!(file, "{},{},{},{:.2},{}", trade_id, ts, recv_ts, latency_ms, machine_id).unwrap();
                    
                    // Flush periódico para garantir que dados não sejam perdidos
                    let count = stats.count.load(Ordering::Relaxed);
                    if count % 1000 == 0 {
                        let _ = file.flush();
                    }
                }
                
                // PASSO 6: Verifica se atingiu o número mínimo de trades
                if min_trades > 0 {
                    let count = stats.count.load(Ordering::Relaxed);
                    if count >= min_trades {
                        if show_realtime {
                            print!("\n\n");
                        }
                        eprintln!("Coleta concluída: {} trades", count);
                        if let Some(ref file) = csv_file {
                            eprintln!("Dados salvos em: {}", file);
                        }
                        break; // Para o loop
                    }
                }
            }
        }
    }
    
    // ========================================================================
    // Estatísticas Finais
    // ========================================================================
    
    if show_realtime {
        print!("\n\n");
    }
    let (count, avg, min, max, p50, p95, p99, jitter, gaps, out_of_order, throughput) = stats.get();
    eprintln!("\n=== Estatísticas Finais ===");
    eprintln!("Machine ID: {}", machine_id);
    eprintln!("Total de trades: {}", count);
    eprintln!("\n--- Latência ---");
    eprintln!("  Média: {:.2}ms", avg);
    eprintln!("  Mediana (p50): {:.2}ms", p50);
    eprintln!("  p95: {:.2}ms", p95);
    eprintln!("  p99: {:.2}ms", p99);
    eprintln!("  Mínima: {:.2}ms", min);
    eprintln!("  Máxima: {:.2}ms", max);
    eprintln!("  Jitter (std): {:.2}ms", jitter);
    eprintln!("\n--- Validações ---");
    eprintln!("  Trades perdidos (gaps): {}", gaps);
    eprintln!("  Trades fora de ordem: {}", out_of_order);
    eprintln!("\n--- Performance ---");
    eprintln!("  Throughput: {:.2} trades/segundo", throughput);
}
