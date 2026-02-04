//! Tipos e estruturas de dados

/// Dados brutos de um trade para salvar no CSV.
#[derive(Debug, Clone)]
pub struct TradeRecord {
    pub trade_id: u64,
    pub ts: u64,           // Timestamp do trade (da Binance)
    pub recv_ts: u64,      // Timestamp de recebimento
    pub latency_ms: f64,   // Latência calculada
    pub machine_id: String,
}

