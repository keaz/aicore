use std::path::Path;
use std::process::{Command, ExitStatus};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxProfile {
    None,
    Ci,
    Strict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxLimits {
    pub profile: String,
    pub cpu_seconds: u64,
    pub memory_bytes: u64,
    pub file_bytes: u64,
    pub max_open_files: u64,
    pub max_processes: u64,
}

impl SandboxProfile {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "none" => Some(Self::None),
            "ci" => Some(Self::Ci),
            "strict" => Some(Self::Strict),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Ci => "ci",
            Self::Strict => "strict",
        }
    }

    pub fn limits(self) -> Option<SandboxLimits> {
        match self {
            Self::None => None,
            Self::Ci => Some(SandboxLimits {
                profile: "ci".to_string(),
                cpu_seconds: 30,
                memory_bytes: 1024 * 1024 * 1024,
                file_bytes: 64 * 1024 * 1024,
                max_open_files: 256,
                max_processes: 64,
            }),
            Self::Strict => Some(SandboxLimits {
                profile: "strict".to_string(),
                cpu_seconds: 5,
                memory_bytes: 256 * 1024 * 1024,
                file_bytes: 8 * 1024 * 1024,
                max_open_files: 64,
                max_processes: 16,
            }),
        }
    }
}

pub fn run_with_limits(
    executable: &Path,
    args: &[String],
    limits: Option<&SandboxLimits>,
) -> anyhow::Result<ExitStatus> {
    if limits.is_none() {
        return Ok(Command::new(executable).args(args).status()?);
    }

    #[cfg(target_os = "linux")]
    {
        let limits = limits.expect("checked above");
        let mut command = Command::new("prlimit");
        command.args(prlimit_args(limits));
        command.arg("--");
        command.arg(executable);
        command.args(args);
        return Ok(command.status()?);
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = executable;
        let _ = args;
        Err(anyhow::anyhow!(
            "sandbox profiles require Linux `prlimit`; use --sandbox none on this platform"
        ))
    }
}

#[cfg(target_os = "linux")]
pub fn prlimit_args(limits: &SandboxLimits) -> Vec<String> {
    vec![
        format!("--cpu={}", limits.cpu_seconds),
        format!("--as={}", limits.memory_bytes),
        format!("--fsize={}", limits.file_bytes),
        format!("--nofile={}", limits.max_open_files),
        format!("--nproc={}", limits.max_processes),
    ]
}

#[cfg(test)]
mod tests {
    use super::SandboxProfile;

    #[test]
    fn parses_known_profiles() {
        assert_eq!(
            SandboxProfile::from_name("none"),
            Some(SandboxProfile::None)
        );
        assert_eq!(SandboxProfile::from_name("ci"), Some(SandboxProfile::Ci));
        assert_eq!(
            SandboxProfile::from_name("strict"),
            Some(SandboxProfile::Strict)
        );
        assert_eq!(SandboxProfile::from_name("unknown"), None);
    }

    #[test]
    fn strict_profile_is_tighter_than_ci() {
        let ci = SandboxProfile::Ci.limits().expect("ci");
        let strict = SandboxProfile::Strict.limits().expect("strict");

        assert!(strict.cpu_seconds < ci.cpu_seconds);
        assert!(strict.memory_bytes < ci.memory_bytes);
        assert!(strict.file_bytes < ci.file_bytes);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_prlimit_args_are_stable() {
        let limits = SandboxProfile::Ci.limits().expect("ci");
        let args = super::prlimit_args(&limits);
        assert_eq!(
            args,
            vec![
                "--cpu=30".to_string(),
                "--as=1073741824".to_string(),
                "--fsize=67108864".to_string(),
                "--nofile=256".to_string(),
                "--nproc=64".to_string(),
            ]
        );
    }
}
