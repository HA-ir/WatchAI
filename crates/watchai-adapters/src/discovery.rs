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
    /// Parse null-delimited `/proc/[pid]/cmdline` bytes into individual arguments.
    pub fn parse_cmdline_args(cmdline_bytes: &[u8]) -> Vec<String> {
        cmdline_bytes
            .split(|&b| b == 0)
            .filter(|slice| !slice.is_empty())
            .map(|slice| String::from_utf8_lossy(slice).trim().to_string())
            .collect()
    }

    /// Check if the parsed argument list matches `target_binaries` while rejecting false positives.
    pub fn matches_cmdline_args(args: &[String], target_binaries: &[&str]) -> bool {
        if args.is_empty() {
            return false;
        }

        let argv0 = &args[0];
        let exe_name = Path::new(argv0)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        // 1. Explicit Rejection List: utilities, shells, inspection tools, build systems
        const REJECTED_EXECUTABLES: &[&str] = &[
            "grep", "rg", "sh", "bash", "zsh", "git", "cargo", "ps", "which", "whereis", "find",
            "sed", "awk", "xargs", "make", "ninja", "strace",
        ];
        if REJECTED_EXECUTABLES.contains(&exe_name) {
            return false;
        }

        // 2. Direct Executable Matching
        for &target in target_binaries {
            if exe_name == target || exe_name == format!("{}-cli", target) {
                return true;
            }
        }

        // 3. Script Runner Matching: node, bun, python, python3, deno executing target script
        const SCRIPT_RUNNERS: &[&str] = &["node", "bun", "python", "python3", "deno", "ts-node"];
        if SCRIPT_RUNNERS.contains(&exe_name) && args.len() > 1 {
            let script_arg = &args[1];
            let script_name = Path::new(script_arg)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            for &target in target_binaries {
                if script_name == target
                    || script_name == format!("{}-cli", target)
                    || script_name.starts_with(&format!("{}.", target))
                    || script_name.starts_with(&format!("{}-cli.", target))
                {
                    return true;
                }
            }
        }

        false
    }

    /// Scan `/proc` for processes matching `binary_name`.
    pub fn scan_processes(
        binary_name: &str,
        provider_id: &'static str,
        provider_display_name: &'static str,
    ) -> Vec<DiscoveredSession> {
        Self::scan_processes_multi(&[binary_name], provider_id, provider_display_name)
    }

    /// Scan `/proc` for processes matching any of `target_binaries`.
    pub fn scan_processes_multi(
        target_binaries: &[&str],
        provider_id: &'static str,
        provider_display_name: &'static str,
    ) -> Vec<DiscoveredSession> {
        let proc_root = watchai_core::liveness::resolve_proc_root();
        Self::scan_proc_dir_multi(
            &proc_root,
            target_binaries,
            provider_id,
            provider_display_name,
        )
    }

    /// Scan a specified proc root directory for matching agent processes.
    pub fn scan_proc_dir(
        proc_root: &Path,
        binary_name: &str,
        provider_id: &'static str,
        provider_display_name: &'static str,
    ) -> Vec<DiscoveredSession> {
        Self::scan_proc_dir_multi(
            proc_root,
            &[binary_name],
            provider_id,
            provider_display_name,
        )
    }

    /// Scan a specified proc root directory for processes matching any of `target_binaries`.
    ///
    /// Isolates per-process I/O errors so that unreadable, locked, or rapidly
    /// disappearing processes do not abort discovery for sibling processes.
    pub fn scan_proc_dir_multi(
        proc_root: &Path,
        target_binaries: &[&str],
        provider_id: &'static str,
        provider_display_name: &'static str,
    ) -> Vec<DiscoveredSession> {
        let mut results = Vec::new();
        let proc_dir = match fs::read_dir(proc_root) {
            Ok(d) => d,
            Err(e) => {
                debug!("Failed to read proc directory '{:?}': {}", proc_root, e);
                return results;
            }
        };

        for entry in proc_dir.flatten() {
            let file_name = entry.file_name();
            let pid_str = match file_name.to_str() {
                Some(s) => s,
                None => continue,
            };

            // Only examine numeric directory names (PIDs); skip named pseudo-dirs like /proc/sys
            let pid: u32 = match pid_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let proc_path = entry.path();
            let cmdline_path = proc_path.join("cmdline");

            // Read /proc/[pid]/cmdline (null-delimited arguments)
            let cmdline_bytes = match fs::read(&cmdline_path) {
                Ok(b) => b,
                Err(e) => {
                    debug!("Skipping PID {}: cannot read cmdline: {}", pid, e);
                    continue; // Process may have exited or permissions denied
                }
            };

            let args = Self::parse_cmdline_args(&cmdline_bytes);
            if !Self::matches_cmdline_args(&args, target_binaries) {
                continue;
            }

            // Inspect working directory via /proc/[pid]/cwd symlink
            let cwd_path = proc_path.join("cwd");
            let project_path = match fs::read_link(&cwd_path) {
                Ok(target) => target,
                Err(e) => {
                    debug!(
                        "Cwd symlink unreadable for PID {}: {}. Falling back to default workspace.",
                        pid, e
                    );
                    PathBuf::from("/unknown/workspace")
                }
            };

            // Get start time from /proc/[pid]/stat (field 22) - must be a reliable, positive value
            let start_time = match Self::read_process_start_time(&proc_path) {
                Some(st) if st > 0 => st,
                _ => {
                    debug!(
                        "Skipping process {}: start time could not be read reliably from /proc/[pid]/stat",
                        pid
                    );
                    continue;
                }
            };

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
                initial_state: LifecycleState::Idle,
                started_at: Utc::now(),
                adapter_status: AdapterStatus::DiscoveryRequired,
                process_start_time: Some(start_time),
            });
        }

        results
    }

    /// Read process start time ticks from /proc/[pid]/stat.
    pub fn read_process_start_time(proc_path: &Path) -> Option<u64> {
        let stat_content = fs::read_to_string(proc_path.join("stat")).ok()?;
        watchai_core::liveness::parse_proc_stat_starttime(&stat_content)
    }
}
