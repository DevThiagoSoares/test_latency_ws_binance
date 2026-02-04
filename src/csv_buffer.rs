//! Buffer Pré-alocado para CSV (1 core)

use std::io::Write;
use std::sync::Mutex;

/// Buffer pré-alocado para escrita de CSV (usado quando há apenas 1 core).
///
/// Usa buffer grande pré-alocado (1MB) para evitar realocações.
/// Escrita no buffer é muito rápida (apenas memória), evitando time-slicing.
pub struct CsvBuffer {
    buffer: Mutex<Vec<u8>>,
    file: Mutex<std::fs::File>,
}

impl CsvBuffer {
    /// Cria novo buffer CSV com arquivo.
    pub fn new(file_path: &str) -> std::io::Result<Self> {
        use std::fs::OpenOptions;
        
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(file_path)?;
        
        // Escreve cabeçalho
        writeln!(file, "trade_id,ts,recv_ts,latency_ms,machine_id")?;
        
        // Pré-aloca buffer de 1MB
        let buffer = Mutex::new(Vec::with_capacity(1024 * 1024));
        
        Ok(Self {
            buffer,
            file: Mutex::new(file),
        })
    }
    
    /// Adiciona linha ao buffer (hot path - apenas escrita em memória).
    pub fn write_line(&self, trade_id: u64, ts: u64, recv_ts: u64, latency_ms: f64, machine_id: &str) {
        let line = format!("{},{},{},{:.2},{}\n", trade_id, ts, recv_ts, latency_ms, machine_id);
        let mut buffer = self.buffer.lock().unwrap();
        buffer.extend_from_slice(line.as_bytes());
    }
    
    /// Faz flush do buffer para disco.
    pub fn flush(&self) -> std::io::Result<()> {
        let mut buffer = self.buffer.lock().unwrap();
        if buffer.is_empty() {
            return Ok(());
        }
        
        let mut file = self.file.lock().unwrap();
        file.write_all(&buffer)?;
        file.flush()?;
        buffer.clear();
        
        Ok(())
    }
    
    /// Flush final (chamado ao finalizar).
    pub fn finalize(&self) -> std::io::Result<()> {
        self.flush()
    }
}

