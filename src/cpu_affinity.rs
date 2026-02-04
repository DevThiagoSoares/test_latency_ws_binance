//! CPU Affinity e Prioridade de Thread

/// Retorna o número de cores CPU disponíveis.
///
/// # Retorno
/// Número de cores (1, 2, 4, etc.)
pub fn get_num_cores() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Define afinidade de CPU para a thread atual.
///
/// # Argumentos
/// * `core_id` - ID do core (0, 1, 2, ...)
///
/// # Retorno
/// `true` se sucesso, `false` se falhou
#[cfg(target_os = "linux")]
pub fn set_cpu_affinity(core_id: usize) -> bool {
    use libc::{cpu_set_t, CPU_SET, CPU_ZERO, sched_setaffinity};
    use std::mem;
    
    unsafe {
        let mut cpuset: cpu_set_t = mem::zeroed();
        CPU_ZERO(&mut cpuset);
        CPU_SET(core_id, &mut cpuset);
        
        let result = sched_setaffinity(
            0, // PID 0 = thread atual
            mem::size_of::<cpu_set_t>(),
            &cpuset,
        );
        
        result == 0
    }
}

#[cfg(not(target_os = "linux"))]
pub fn set_cpu_affinity(_core_id: usize) -> bool {
    // Não suportado em outros sistemas
    false
}

/// Define prioridade da thread atual.
///
/// # Argumentos
/// * `priority` - Prioridade (maior = mais prioritário, típico: -20 a 19)
///
/// # Retorno
/// `true` se sucesso, `false` se falhou
#[cfg(target_os = "linux")]
pub fn set_thread_priority(priority: i32) -> bool {
    use libc::{setpriority, PRIO_PROCESS};
    
    unsafe {
        let result = setpriority(PRIO_PROCESS, 0, priority);
        result == 0
    }
}

#[cfg(not(target_os = "linux"))]
pub fn set_thread_priority(_priority: i32) -> bool {
    // Não suportado em outros sistemas
    false
}

