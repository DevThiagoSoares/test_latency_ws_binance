//! Teste de Latência - Binance WebSocket Trades
//!
//! Este programa conecta ao WebSocket da Binance, recebe trades de BTC/USDT em tempo real,
//! e mede a latência entre o momento que o trade aconteceu e quando foi recebido.
//!
//! OTIMIZAÇÕES DE PERFORMANCE:
//! - I/O movido para thread separada (fora do hot path)
//! - Sem cálculo de estatísticas durante coleta (apenas no final, após JOIN)
//! - Hot path mínimo: receber → extrair → calcular → enviar para channel
//!
//! Uso:
//!   MACHINE_ID=m8a.xlarge ./target/release/binance-trades
//!   CSV_FILE=latency.csv MACHINE_ID=m8a.xlarge MIN_TRADES=100000 ./target/release/binance-trades

use futures_util::StreamExt;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::SystemTime;
use tokio_tungstenite::{connect_async, tungstenite::Message};

// ============================================================================
// Estrutura de Dados Brutos
// ============================================================================

/// Dados brutos de um trade para salvar no CSV.
/// 
/// Esta estrutura é enviada para a thread de I/O via channel,
/// mantendo o hot path livre de operações de I/O.
#[derive(Debug, Clone)]
struct TradeRecord {
    trade_id: u64,
    ts: u64,           // Timestamp do trade (da Binance)
    recv_ts: u64,      // Timestamp de recebimento
    latency_ms: f64,   // Latência calculada
    machine_id: String,
}

// ============================================================================
// Extração de Dados do JSON (Hot Path)
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
// Thread de I/O (Fora do Hot Path)
// ============================================================================

/// Thread dedicada para escrever dados no CSV.
///
/// Esta thread roda separadamente do hot path, evitando que operações de I/O
/// (que podem ter locks internos do runtime/stdio) adicionem latência à medição.
///
/// Usa buffer interno e flush periódico para balancear performance e segurança.
fn csv_writer_thread(
    csv_file: String,
    _machine_id: String,
    rx: mpsc::Receiver<TradeRecord>,
) {
    use std::fs::OpenOptions;
    
    // Abre arquivo CSV
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&csv_file)
        .expect(&format!("Erro ao criar CSV: {}", csv_file));
    
    // Escreve cabeçalho
    writeln!(file, "trade_id,ts,recv_ts,latency_ms,machine_id").unwrap();
    
    let mut count = 0u64;
    let mut buffer = Vec::with_capacity(8192); // Buffer de 8KB
    
    // Loop: recebe dados do channel e escreve no arquivo
    while let Ok(record) = rx.recv() {
        // Formata linha CSV
        let line = format!("{},{},{},{:.2},{}\n", 
            record.trade_id, 
            record.ts, 
            record.recv_ts, 
            record.latency_ms, 
            record.machine_id
        );
        
        // Adiciona ao buffer
        buffer.extend_from_slice(line.as_bytes());
        
        count += 1;
        
        // Flush periódico: a cada 1000 trades ou se buffer > 8KB
        if count % 1000 == 0 || buffer.len() >= 8192 {
            file.write_all(&buffer).unwrap();
            file.flush().unwrap();
            buffer.clear();
        }
    }
    
    // Flush final do buffer restante
    if !buffer.is_empty() {
        file.write_all(&buffer).unwrap();
        file.flush().unwrap();
    }
    
    eprintln!("CSV writer finalizado: {} trades salvos em {}", count, csv_file);
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
    
    // ========================================================================
    // Setup de I/O em Thread Separada (se CSV habilitado)
    // ========================================================================
    
    let (csv_tx, csv_rx) = if csv_file.is_some() {
        let (tx, rx) = mpsc::channel();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };
    
    // Spawn thread de I/O (fora do hot path)
    if let (Some(file), Some(rx)) = (csv_file.clone(), csv_rx) {
        let machine_id_io = machine_id.clone();
        std::thread::spawn(move || {
            csv_writer_thread(file, machine_id_io, rx);
        });
    }
    
    // ========================================================================
    // Contador Simples (apenas para display, sem estatísticas complexas)
    // ========================================================================
    
    let count = std::sync::Arc::new(AtomicU64::new(0));
    let count_display = count.clone();
    let machine_id_display = machine_id.clone();
    
    if show_realtime {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            loop {
                interval.tick().await;
                let c = count_display.load(Ordering::Relaxed);
                if c > 0 {
                    print!("\r\x1b[K"); // ANSI: volta ao início da linha e limpa
                    print!("[{}] Trades coletados: {}", machine_id_display, c);
                    let _ = std::io::stdout().flush();
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
    if csv_file.is_some() {
        eprintln!("CSV: {}", csv_file.as_ref().unwrap());
    }
    if show_realtime {
        eprintln!("Display tempo real: ATIVADO\n");
    } else {
        eprintln!("Display tempo real: DESATIVADO\n");
    }

    let (ws_stream, _) = connect_async(url).await.expect("Erro ao conectar");
    eprintln!("Conectado! Coletando dados...\n");

    let (_write, mut read) = ws_stream.split();

    // ========================================================================
    // HOT PATH: Loop Principal (Mínimo possível)
    // ========================================================================
    //
    // Este é o caminho crítico de performance. Qualquer operação aqui
    // adiciona latência à medição. Por isso:
    // - Sem I/O (movido para thread separada)
    // - Sem estatísticas complexas (apenas contador atômico)
    // - Apenas: receber → extrair → calcular → enviar para channel
    
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
                
                // PASSO 4: Atualiza contador (lock-free, muito rápido)
                count.fetch_add(1, Ordering::Relaxed);
                
                // PASSO 5: Envia para thread de I/O (se habilitado)
                // Channel unbounded é muito rápido (apenas push em queue)
                if let Some(ref tx) = csv_tx {
                    let record = TradeRecord {
                        trade_id,
                        ts,
                        recv_ts,
                        latency_ms,
                        machine_id: machine_id.clone(),
                    };
                    // Ignora erro se receiver foi fechado (thread finalizou)
                    let _ = tx.send(record);
                }
                
                // PASSO 6: Verifica se atingiu o número mínimo de trades
                if min_trades > 0 {
                    let c = count.load(Ordering::Relaxed);
                    if c >= min_trades {
                        if show_realtime {
                            print!("\n\n");
                        }
                        eprintln!("Coleta concluída: {} trades", c);
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
    // Finalização
    // ========================================================================
    
    if show_realtime {
        print!("\n\n");
    }
    
    let total = count.load(Ordering::Relaxed);
    eprintln!("\n=== Coleta Finalizada ===");
    eprintln!("Machine ID: {}", machine_id);
    eprintln!("Total de trades coletados: {}", total);
    
    if let Some(ref file) = csv_file {
        eprintln!("Dados salvos em: {}", file);
        eprintln!("\n💡 Próximo passo: Faça JOIN dos CSVs por trade_id para análise estatística");
    }
    
    // Fecha channel para finalizar thread de I/O
    drop(csv_tx);
    
    // Aguarda um pouco para thread de I/O finalizar
    std::thread::sleep(std::time::Duration::from_millis(100));
}
