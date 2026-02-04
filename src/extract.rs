//! Extração de Dados do JSON (Hot Path)

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
pub fn extract_trade_data(text: &str) -> Option<(u64, u64)> {
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

