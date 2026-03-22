use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn enabled() -> bool {
    match std::env::var("GEN_CALLGRAPH_TRACE") {
        Ok(v) => {
            let v = v.to_lowercase();
            v == "1" || v == "true" || v == "on" || v == "yes"
        }
        Err(_) => false,
    }
}

pub fn log(component: &str, event: &str, message: &str) {
    if !enabled() {
        return;
    }
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    let log_line = format!(
        "[trace ts={} component={} event={}] {}\n",
        timestamp_ms, component, event, message
    );
    let log_path = std::env::var("GEN_CALLGRAPH_TRACE_FILE")
        .unwrap_or_else(|_| String::from("gen_callgraph.log"));

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = file.write_all(log_line.as_bytes());
    }
}
