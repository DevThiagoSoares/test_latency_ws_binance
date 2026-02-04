//! Teste de Latência - Binance WebSocket Trades
//!
//! Este programa conecta ao WebSocket da Binance, recebe trades de BTC/USDT em tempo real,
//! e mede a latência entre o momento que o trade aconteceu e quando foi recebido.
//!
//! OTIMIZAÇÕES DE PERFORMANCE:
//! - Thread separada para I/O com CPU affinity (core dedicado)
//! - Coleta no core 0, salvamento no core 1 (quando disponível)
//! - Buffer pré-alocado para evitar realocações
//! - Sem cálculo de estatísticas durante coleta (apenas contador)
//! - Hot path mínimo: receber → extrair → calcular → enviar para channel
//!
//! Uso:
//!   MACHINE_ID=m8a.xlarge ./target/release/binance-trades
//!   CSV_FILE=latency.csv MACHINE_ID=m8a.xlarge MIN_TRADES=100000 ./target/release/binance-trades

mod cpu_affinity;
mod csv_buffer;
mod csv_writer;
mod extract;
mod types;

use cpu_affinity::{get_num_cores, set_cpu_affinity, set_thread_priority};
use csv_buffer::CsvBuffer;
use csv_writer::csv_writer_thread;
use extract::extract_trade_data;
use futures_util::StreamExt;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::SystemTime;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use types::TradeRecord;

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
    // Detecta número de cores e escolhe estratégia
    // ========================================================================
    
    let num_cores = get_num_cores();
    eprintln!("Cores disponíveis: {}", num_cores);
    
    // ========================================================================
    // Define CPU Affinity para Thread Principal (Core 0)
    // ========================================================================
    
    // Tenta definir core 0 para coleta (thread principal)
    if set_cpu_affinity(0) {
        eprintln!("Thread principal: CPU affinity definida para core 0");
    }
    
    // Define prioridade alta para coleta
    if set_thread_priority(-10) {
        eprintln!("Thread principal: Prioridade alta definida (-10)");
    }
    
    // ========================================================================
    // Setup de I/O: Thread Separada (2+ cores) ou Buffer Pré-alocado (1 core)
    // ========================================================================
    
    // Para 2+ cores: usa thread separada com CPU affinity
    // Para 1 core: usa buffer pré-alocado (evita time-slicing)
    let (csv_tx, csv_rx) = if csv_file.is_some() && num_cores >= 2 {
        let (tx, rx) = mpsc::channel();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };
    
    let csv_buffer: Option<std::sync::Arc<CsvBuffer>> = if csv_file.is_some() && num_cores == 1 {
        match CsvBuffer::new(csv_file.as_ref().unwrap()) {
            Ok(buffer) => {
                eprintln!("Usando buffer pré-alocado (1 core detectado - evita time-slicing)");
                Some(std::sync::Arc::new(buffer))
            },
            Err(e) => {
                eprintln!("Erro ao criar CSV: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        None
    };
    
    // Spawn thread de I/O com CPU affinity (core 1) - apenas se 2+ cores
    if let (Some(file), Some(rx)) = (csv_file.clone(), csv_rx) {
        let machine_id_io = machine_id.clone();
        eprintln!("Usando thread separada para I/O (2+ cores detectados)");
        std::thread::spawn(move || {
            csv_writer_thread(file, machine_id_io, rx);
        });
    }
    
    // ========================================================================
    // Contador Simples (apenas para display)
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
    // - Thread separada para I/O (core 1, prioridade menor)
    // - Sem estatísticas complexas (apenas contador atômico)
    // - Channel unbounded (muito rápido, apenas push em queue)
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
                let current_count = count.fetch_add(1, Ordering::Relaxed) + 1;
                
                // PASSO 5: Salva no CSV (estratégia depende do número de cores)
                // 1 core: escreve direto no buffer pré-alocado (evita time-slicing)
                // 2+ cores: envia para thread separada via channel
                if let Some(ref buffer) = csv_buffer {
                    // 1 core: buffer pré-alocado
                    buffer.write_line(trade_id, ts, recv_ts, latency_ms, &machine_id);
                    
                    // Flush periódico: a cada 1000 trades
                    if current_count % 1000 == 0 {
                        if let Err(e) = buffer.flush() {
                            eprintln!("Erro ao fazer flush: {}", e);
                        }
                    }
                } else if let Some(ref tx) = csv_tx {
                    // 2+ cores: thread separada
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
                if min_trades > 0 && current_count >= min_trades {
                    if show_realtime {
                        print!("\n\n");
                    }
                    eprintln!("Coleta concluída: {} trades", current_count);
                    if let Some(ref file) = csv_file {
                        eprintln!("Dados salvos em: {}", file);
                    }
                    break; // Para o loop
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
    
    // Finaliza I/O dependendo da estratégia usada
    if csv_buffer.is_some() {
        // 1 core: flush final do buffer
        if let Some(ref buffer) = csv_buffer {
            if let Err(e) = buffer.finalize() {
                eprintln!("Erro ao finalizar CSV: {}", e);
            }
        }
    } else if csv_tx.is_some() {
        // 2+ cores: fecha channel e aguarda thread finalizar
        drop(csv_tx);
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    
    if let Some(ref file) = csv_file {
        eprintln!("Dados salvos em: {}", file);
        eprintln!("\n💡 Próximo passo: Faça JOIN dos CSVs por trade_id para análise estatística");
    }
}
