pub fn os() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}
pub fn host_model() -> String { "Unknown".into() }
pub fn packages() -> Vec<String> { Vec::new() }
pub fn desktop_environment() -> Option<String> { None }
pub fn window_manager() -> Option<String> { None }
pub fn resolution() -> Option<String> { None }
pub fn gpus() -> Vec<String> { Vec::new() }
pub fn battery() -> Option<String> { None }
pub fn init_system() -> Option<String> { None }
pub fn load_average() -> Option<String> { None }
pub fn cpu_temperature() -> Option<String> { None }
pub fn gpu_drivers() -> Vec<String> { Vec::new() }
pub fn audio_server() -> Option<String> { None }
pub fn session_type() -> Option<String> { None }
pub fn local_ip() -> Option<String> { None }
pub fn local_ip6() -> Option<String> { None }
pub fn theme() -> Option<String> { None }
