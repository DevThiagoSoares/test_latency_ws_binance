//! Thread de I/O com CPU Affinity (2+ cores)

use crate::cpu_affinity::{set_cpu_affinity, set_thread_priority};
use crate::types::TradeRecord;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::mpsc;

/// Thread dedicada para escrever dados no CSV.
///
/// Esta thread roda em um core separado (core 1) com prioridade menor,
/// evitando interferência no hot path de coleta (core 0).
///
/// Usa buffer interno e flush periódico para balancear performance e segurança.
pub fn csv_writer_thread(
    csv_file: String,
    _machine_id: String,
    rx: mpsc::Receiver<TradeRecord>,
) {
    // Define CPU affinity: core 1 para I/O (se disponível)
    if set_cpu_affinity(1) {
        eprintln!("I/O thread: CPU affinity definida para core 1");
    } else {
        eprintln!("I/O thread: CPU affinity não disponível (usando core padrão)");
    }
    
    // Define prioridade menor para I/O (não interfere na coleta)
    if set_thread_priority(10) {
        eprintln!("I/O thread: Prioridade definida (10)");
    }
    
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
    let mut buffer = Vec::with_capacity(1024 * 1024); // Buffer de 1MB
    
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
        
        // Flush periódico: a cada 1000 trades ou se buffer > 1MB
        if count % 1000 == 0 || buffer.len() >= 1024 * 1024 {
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

