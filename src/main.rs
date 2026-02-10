//! Teste de Latência - Binance WebSocket Trades
//!
//! Este programa conecta ao WebSocket da Binance, recebe trades de BTC/USDT em tempo real,
//! e mede a latência entre o momento que o trade aconteceu e quando foi recebido.
//!
//! OTIMIZAÇÕES DE PERFORMANCE:
//! - Calibração de clock vs Binance (corrige drift entre máquinas)
//! - Precisão em microssegundos (necessário para comparação entre instâncias)
//! - TCP_NODELAY (reduz latência de rede)
//! - ClockRef (evita syscalls repetidos usando Instant monotônico)
//! - Parsing JSON zero-allocation (busca direta em bytes)
//! - Tudo em memória durante coleta (zero I/O no hot path)
//! - Single-thread (current_thread runtime)
//!
//! Uso:
//!   MACHINE_ID=m8a.xlarge cargo run --release
//!   MACHINE_ID=m8a.xlarge cargo run --release -- btcusdt 100000
//!   CSV_FILE=latency.csv MACHINE_ID=m8a.xlarge cargo run --release -- btcusdt 100000

use std::io::Write;
use std::time::{Duration, Instant, SystemTime};

use futures_util::StreamExt;
use tokio::net::TcpSocket;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

const DEFAULT_SYMBOL: &str = "btcusdt";
const DEFAULT_COUNT: usize = 100_000;

// ---------------------------------------------------------------------------
// High Precision Timestamp
// ---------------------------------------------------------------------------

/// Monotonic reference to convert Instant -> epoch micros without syscall.
struct ClockRef {
    instant: Instant,
    epoch_us: u64,
}

impl ClockRef {
    fn new() -> Self {
        // Capture both as close as possible
        let instant = Instant::now();
        let epoch_us = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        Self { instant, epoch_us }
    }

    /// Converts an Instant to epoch microseconds without syscall.
    #[inline(always)]
    fn to_epoch_us(&self, now: Instant) -> u64 {
        let elapsed = now.duration_since(self.instant).as_micros() as u64;
        self.epoch_us + elapsed
    }
}

// ---------------------------------------------------------------------------
// Manual JSON Parse (zero-alloc)
// ---------------------------------------------------------------------------

/// Extracts "t" (trade_id) and "T" (trade_ts_ms) from Binance JSON.
/// Note: Binance sends "T" in milliseconds; we convert to microseconds later for CSV/storage.
/// Manual parse without allocation — searches directly for numeric fields.
#[inline(always)]
fn parse_trade_fast(json: &[u8]) -> Option<(u64, u64)> {
    let trade_id = extract_u64_field(json, b"\"t\":")?;
    let trade_ts = extract_u64_field(json, b"\"T\":")?;
    Some((trade_id, trade_ts))
}

/// Searches for a numeric field in JSON by pattern `"key":`.
/// Assumes value is an integer without quotes (true for "t" and "T" from Binance).
/// Returns the number as-is (no unit conversion here).
#[inline(always)]
fn extract_u64_field(json: &[u8], pattern: &[u8]) -> Option<u64> {
    let pos = find_pattern(json, pattern)?;
    let start = pos + pattern.len();

    // Skip optional spaces
    let mut i = start;
    while i < json.len() && json[i] == b' ' {
        i += 1;
    }

    // Parse number
    let mut val: u64 = 0;
    while i < json.len() {
        let b = json[i];
        if b >= b'0' && b <= b'9' {
            val = val * 10 + (b - b'0') as u64;
            i += 1;
        } else {
            break;
        }
    }

    if i > start {
        Some(val)
    } else {
        None
    }
}

/// Searches for a byte pattern inside a slice.
#[inline(always)]
fn find_pattern(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > haystack.len() {
        return None;
    }
    let limit = haystack.len() - needle.len();
    for i in 0..=limit {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Trade Data
// ---------------------------------------------------------------------------

struct Trade {
    trade_id: u64,
    trade_ts_us: u64,    // trade timestamp (Binance, microseconds)
    recv_ts_us: u64,     // receive timestamp (local, microseconds)
    latency_us: i64,     // difference in microseconds (can be negative if clock drift)
}

// ---------------------------------------------------------------------------
// Clock Calibration via Binance REST API
// ---------------------------------------------------------------------------

/// Measures local clock offset vs Binance by making N requests to /api/v3/time.
/// Returns estimated offset in microseconds (local - server).
/// 
/// NOTE: Reduzido para 10-50 amostras para não demorar muito (1000 = ~100 segundos).
async fn calibrate_clock(n: usize) -> i64 {
    let n = n.min(50); // Limita a 50 amostras máximo
    eprintln!("Calibrating clock against Binance ({} samples)...", n);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("Error creating HTTP client");

    let mut offsets = Vec::with_capacity(n);

    for _ in 0..n {
        let t1_us = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as i64;

        let resp = client
            .get("https://api.binance.com/api/v3/time")
            .send()
            .await;

        let t3_us = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as i64;

        if let Ok(resp) = resp {
            if let Ok(body) = resp.bytes().await {
                // {"serverTime":1234567890123}
                if let Some(server_ms) = extract_u64_field(&body, b"\"serverTime\":") {
                    let server_us = server_ms as i64 * 1000;
                    let rtt_us = t3_us - t1_us;
                    // Estimates server timestamp is at RTT/2
                    let local_at_server = t1_us + rtt_us / 2;
                    let offset = local_at_server - server_us;
                    offsets.push((offset, rtt_us));
                }
            }
        }
        // Sleep menor para acelerar calibração (mas ainda permite múltiplas amostras)
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    if offsets.is_empty() {
        eprintln!("  WARNING: Could not calibrate. Using offset = 0");
        return 0;
    }

    // Use sample with lowest RTT (most accurate)
    offsets.sort_by_key(|&(_, rtt)| rtt);
    let best = offsets[0];
    let median_idx = offsets.len() / 2;
    let median = offsets[median_idx];

    eprintln!("  Best RTT: {}µs, offset: {}µs", best.1, best.0);
    eprintln!("  Median RTT: {}µs, offset: {}µs", median.1, median.0);
    eprintln!(
        "  Local clock is ~{:.2}ms {} from Binance",
        best.0.abs() as f64 / 1000.0,
        if best.0 > 0 { "ahead" } else { "behind" }
    );

    best.0
}

// ---------------------------------------------------------------------------
// WebSocket Connection with TCP_NODELAY
// ---------------------------------------------------------------------------

async fn connect_ws(
    url: &str,
) -> WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let request = url.into_client_request().expect("Invalid URL");
    let domain = request.uri().host().unwrap().to_string();
    let port = request.uri().port_u16().unwrap_or(9443);

    // Resolve DNS
    let addr = tokio::net::lookup_host(format!("{}:{}", domain, port))
        .await
        .expect("DNS Error")
        .next()
        .expect("No IP address");

    // Create socket with TCP_NODELAY
    let socket = TcpSocket::new_v4().expect("Error creating socket");
    socket.set_nodelay(true).expect("Error setting TCP_NODELAY");

    let tcp_stream = socket.connect(addr).await.expect("Error connecting TCP");

    // TLS + WebSocket handshake
    let (ws, _) = tokio_tungstenite::client_async_tls(request, tcp_stream)
        .await
        .expect("WebSocket handshake error");

    ws
}

// ---------------------------------------------------------------------------
// Save CSV
// ---------------------------------------------------------------------------

fn save_csv(path: &str, trades: &[Trade], label: &str, machine_id: &str, clock_offset_us: i64) -> std::io::Result<()> {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(
        file,
        "label,machine_id,trade_id,trade_ts_us,recv_ts_us,latency_us,clock_offset_us"
    )?;
    for t in trades {
        writeln!(
            file,
            "{},{},{},{},{},{},{}",
            label,
            machine_id,
            t.trade_id,
            t.trade_ts_us,
            t.recv_ts_us,
            t.latency_us,
            clock_offset_us,
        )?;
    }
    file.flush()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    let symbol = args.get(1).map(|s| s.as_str()).unwrap_or(DEFAULT_SYMBOL);
    let count: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_COUNT);
    // Optional label passed via CLI: <symbol> <count> [label]
    let label: String = args.get(3).cloned().unwrap_or_else(|| "unknown".to_string());
    
    // Machine ID via variável de ambiente (essencial para múltiplas instâncias)
    let machine_id = std::env::var("MACHINE_ID")
        .or_else(|_| std::env::var("AWS_REGION"))
        .unwrap_or_else(|_| "unknown".to_string());
    
    // Arquivo de saída único por instância (evita conflitos)
    let output_file = std::env::var("CSV_FILE")
        .unwrap_or_else(|_| format!("trades_{}_{}.csv", machine_id, 
            SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()));

    eprintln!("=== Binance Latency Benchmark ===");
    eprintln!("Label:      {}", label);
    eprintln!("Machine ID: {}", machine_id);
    eprintln!("Symbol:     {}", symbol.to_uppercase());
    eprintln!("Trades:     {}", count);
    eprintln!("Output:     {}", output_file);

    // --- Clock Calibration ---
    // Reduzido para 20 amostras (suficiente e rápido: ~1 segundo)
    let clock_offset_us = calibrate_clock(20).await;

    // --- Clock reference (monotonic -> epoch without syscall) ---
    let clock_ref = ClockRef::new();

    // --- Pre-allocate buffer ---
    let mut trades: Vec<Trade> = Vec::with_capacity(count);

    // --- Connect to WebSocket with TCP_NODELAY ---
    let url = format!(
        "wss://stream.binance.com:9443/ws/{}@trade",
        symbol.to_lowercase()
    );
    eprintln!("Connecting to {}...", url);

    let ws = connect_ws(&url).await;
    let (_write, mut read) = ws.split();

    eprintln!("Connected! Collecting {} trades...", count);

    // --- Collection Loop ---
    while let Some(msg) = read.next().await {
        // Timestamp IMMEDIATELY — before any processing
        let recv_instant = Instant::now();

        let data = match &msg {
            Ok(Message::Text(text)) => text.as_bytes(),
            Ok(Message::Binary(bin)) => bin.as_slice(),
            _ => continue,
        };

        // Zero-alloc parse
        if let Some((trade_id, trade_ts_ms)) = parse_trade_fast(data) {
            // Validação básica: ignora trades inválidos
            if trade_id == 0 || trade_ts_ms == 0 {
                continue;
            }
            
            let recv_ts_us = clock_ref.to_epoch_us(recv_instant);
            let trade_ts_us: u64 = trade_ts_ms * 1000;
            let latency_us = recv_ts_us as i64 - trade_ts_us as i64 - clock_offset_us;

            trades.push(Trade {
                trade_id,
                trade_ts_us,
                recv_ts_us,
                latency_us,
            });

            // Para quando buffer estiver cheio
            if trades.len() >= count {
                break;
            }
        }
    }

    eprintln!("Collection finished: {} trades", trades.len());
    
    // --- Estatísticas de Latência ---
    if !trades.is_empty() {
        let latencies: Vec<i64> = trades.iter().map(|t| t.latency_us).collect();
        let mut sorted = latencies.clone();
        sorted.sort();
        
        let min = sorted[0];
        let max = sorted[sorted.len() - 1];
        let median = sorted[sorted.len() / 2];
        let p95 = sorted[(sorted.len() as f64 * 0.95) as usize];
        let p99 = sorted[(sorted.len() as f64 * 0.99) as usize];
        
        eprintln!("\n=== Latency Statistics ===");
        eprintln!("Min:    {}µs", min);
        eprintln!("Max:    {}µs", max);
        eprintln!("Median: {}µs", median);
        eprintln!("P95:    {}µs", p95);
        eprintln!("P99:    {}µs", p99);
    }

    // --- Save CSV ---
    match save_csv(&output_file, &trades, &label, &machine_id, clock_offset_us) {
        Ok(()) => eprintln!("\n✅ Data saved to: {}", output_file),
        Err(e) => eprintln!("\n❌ Error saving CSV: {}", e),
    }
    
    eprintln!("\n💡 Próximo passo: Faça JOIN dos CSVs por trade_id para análise comparativa");
}
