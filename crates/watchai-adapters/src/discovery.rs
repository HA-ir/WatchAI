use crate::traits::DiscoveredSession;
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, trace};
use watchai_core::session::{derive_process_session_id, AdapterStatus};
use watchai_core::state::LifecycleState;

/// Scans the local Linux `/proc` filesystem to discover agent processes matching a binary name pattern.
pub struct ProcessScanner;

impl ProcessScanner {
    /// Scan `/proc` for processes whose cmdline contains `binary_name`.
    pub fn scan_processes(
        binary_name: &str,
        provider_id: &'static str,
        provider_display_name: &'static str,
    ) -> Vec<DiscoveredSession> {
        let mut results = Vec::new();
        let proc_dir = match fs::read_dir("/proc") {
            Ok(d) => d,
            Err(e) => {
                debug!("Failed to read /proc directory: {}", e);
                return results;
            }
        };

        for entry in proc_dir.flatten() {
            let file_name = entry.file_name();
            let pid_str = match file_name.to_str() {
                Some(s) => s,
                None => continue,
            };

            // Only examine numeric directory names (PIDs)
            let pid: u32 = match pid_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let proc_path = entry.path();
            let cmdline_path = proc_path.join("cmdline");

            // Read /proc/[pid]/cmdline (null-delimited arguments)
            let cmdline_bytes = match fs::read(&cmdline_path) {
                Ok(b) => b,
                Err(_) => continue, // Process may have exited or permissions denied
            };

            let cmdline = String::from_utf8_lossy(&cmdline_bytes);
            if !cmdline.contains(binary_name) {
                continue;
            }

            // Inspect working directory via /proc/[pid]/cwd symlink
            let cwd_path = proc_path.join("cwd");
            let project_path = match fs::read_link(&cwd_path) {
                Ok(target) => target,
                Err(_) => PathBuf::from("/unknown/workspace"),
            };

            // Get start time from /proc/[pid]/stat (field 22)
            let start_time = Self::read_process_start_time(&proc_path).unwrap_or(0);

            let session_id = derive_process_session_id(pid, start_time, &project_path);
            trace!(
                "Discovered agent process: pid={}, provider={}, path={:?}",
                pid,
                provider_id,
                project_path
            );

            results.push(DiscoveredSession {
                session_id,
                provider_id: provider_id.to_string(),
                provider_display_name: provider_display_name.to_string(),
                project_path,
                process_id: Some(pid),
                initial_state: LifecycleState::Working,
                started_at: Utc::now(),
                adapter_status: AdapterStatus::DiscoveryRequired,
            });
        }

        results
    }

    /// Read process start time ticks from /proc/[pid]/stat.
    fn read_process_start_time(proc_path: &Path) -> Option<u64> {
        let stat_content = fs::read_to_string(proc_path.join("stat")).ok()?;
        // The stat file format has comm in parentheses, e.g. "123 (claude) S 1 ..."
        let close_paren_idx = stat_content.rfind(')')?;
        let rest = &stat_content[close_paren_idx + 1..];
        let fields: Vec<&str> = rest.split_whitespace().collect();
        // starttime is the 20th field after comm (field 22 overall, 0-indexed as field 19 after ')')
        fields.get(19).and_then(|val| val.parse::<u64>().ok())
    }
}
