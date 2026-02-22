use std::fs;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxPermissions {
    pub fs: bool,
    pub net: bool,
    pub proc: bool,
    pub time: bool,
}

impl SandboxPermissions {
    fn allow_all() -> Self {
        Self {
            fs: true,
            net: true,
            proc: true,
            time: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxPolicy {
    pub profile: String,
    #[serde(default)]
    pub limits: Option<SandboxLimits>,
    #[serde(default = "SandboxPermissions::allow_all")]
    pub permissions: SandboxPermissions,
}

impl SandboxPolicy {
    fn validate(&self) -> anyhow::Result<()> {
        if self.profile.trim().is_empty() {
            anyhow::bail!("sandbox policy requires non-empty `profile`");
        }
        if let Some(limits) = &self.limits {
            if limits.cpu_seconds == 0
                || limits.memory_bytes == 0
                || limits.file_bytes == 0
                || limits.max_open_files == 0
                || limits.max_processes == 0
            {
                anyhow::bail!(
                    "sandbox policy limits must be positive for cpu/memory/file/open_files/processes"
                );
            }
        }
        Ok(())
    }
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

    pub fn policy(self) -> SandboxPolicy {
        match self {
            Self::None => SandboxPolicy {
                profile: "none".to_string(),
                limits: None,
                permissions: SandboxPermissions::allow_all(),
            },
            Self::Ci => SandboxPolicy {
                profile: "ci".to_string(),
                limits: Some(SandboxLimits {
                    profile: "ci".to_string(),
                    cpu_seconds: 30,
                    memory_bytes: 1024 * 1024 * 1024,
                    file_bytes: 64 * 1024 * 1024,
                    max_open_files: 256,
                    max_processes: 64,
                }),
                permissions: SandboxPermissions {
                    fs: true,
                    net: false,
                    proc: false,
                    time: true,
                },
            },
            Self::Strict => SandboxPolicy {
                profile: "strict".to_string(),
                limits: Some(SandboxLimits {
                    profile: "strict".to_string(),
                    cpu_seconds: 5,
                    memory_bytes: 256 * 1024 * 1024,
                    file_bytes: 8 * 1024 * 1024,
                    max_open_files: 64,
                    max_processes: 16,
                }),
                permissions: SandboxPermissions {
                    fs: false,
                    net: false,
                    proc: false,
                    time: false,
                },
            },
        }
    }

    pub fn limits(self) -> Option<SandboxLimits> {
        self.policy().limits
    }
}

pub fn load_policy(path: &Path) -> anyhow::Result<SandboxPolicy> {
    let raw = fs::read_to_string(path)?;
    let policy = serde_json::from_str::<SandboxPolicy>(&raw)?;
    policy.validate()?;
    Ok(policy)
}

pub fn run_with_limits(
    executable: &Path,
    args: &[String],
    limits: Option<&SandboxLimits>,
) -> anyhow::Result<ExitStatus> {
    let policy = SandboxPolicy {
        profile: limits
            .map(|value| value.profile.clone())
            .unwrap_or_else(|| "none".to_string()),
        limits: limits.cloned(),
        permissions: SandboxPermissions::allow_all(),
    };
    run_with_policy(executable, args, &policy)
}

pub fn run_with_policy(
    executable: &Path,
    args: &[String],
    policy: &SandboxPolicy,
) -> anyhow::Result<ExitStatus> {
    policy.validate()?;

    if policy.limits.is_none() {
        let mut command = Command::new(executable);
        command.args(args);
        apply_policy_env(&mut command, policy);
        return Ok(command.status()?);
    }

    #[cfg(target_os = "linux")]
    {
        let limits = policy.limits.as_ref().expect("checked above");
        let mut command = Command::new("prlimit");
        command.args(prlimit_args(limits));
        command.arg("--");
        command.arg(executable);
        command.args(args);
        apply_policy_env(&mut command, policy);
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

fn apply_policy_env(command: &mut Command, policy: &SandboxPolicy) {
    command
        .env("AIC_SANDBOX_PROFILE", &policy.profile)
        .env("AIC_SANDBOX_ALLOW_FS", bool_env(policy.permissions.fs))
        .env("AIC_SANDBOX_ALLOW_NET", bool_env(policy.permissions.net))
        .env("AIC_SANDBOX_ALLOW_PROC", bool_env(policy.permissions.proc))
        .env("AIC_SANDBOX_ALLOW_TIME", bool_env(policy.permissions.time))
        .env("AIC_SANDBOX_DIAGNOSTIC_JSON", "1");
}

fn bool_env(value: bool) -> &'static str {
    if value {
        "1"
    } else {
        "0"
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
    use std::fs;

    use tempfile::tempdir;

    use super::{load_policy, SandboxProfile};

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

    #[test]
    fn built_in_profile_permissions_match_expected_policies() {
        let ci = SandboxProfile::Ci.policy();
        assert!(ci.permissions.fs);
        assert!(!ci.permissions.net);
        assert!(!ci.permissions.proc);
        assert!(ci.permissions.time);

        let strict = SandboxProfile::Strict.policy();
        assert!(!strict.permissions.fs);
        assert!(!strict.permissions.net);
        assert!(!strict.permissions.proc);
        assert!(!strict.permissions.time);
    }

    #[test]
    fn loads_custom_policy_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("sandbox-policy.json");
        fs::write(
            &path,
            r#"{
  "profile": "custom-ci",
  "permissions": { "fs": true, "net": false, "proc": false, "time": true },
  "limits": {
    "profile": "custom-ci",
    "cpu_seconds": 20,
    "memory_bytes": 536870912,
    "file_bytes": 16777216,
    "max_open_files": 128,
    "max_processes": 32
  }
}"#,
        )
        .expect("write policy");

        let policy = load_policy(&path).expect("load policy");
        assert_eq!(policy.profile, "custom-ci");
        assert!(policy.permissions.fs);
        assert!(!policy.permissions.net);
        assert!(policy.limits.is_some());
    }

    #[test]
    fn rejects_invalid_custom_policy() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("bad-policy.json");
        fs::write(
            &path,
            r#"{
  "profile": "",
  "permissions": { "fs": true, "net": true, "proc": true, "time": true }
}"#,
        )
        .expect("write policy");

        let err = load_policy(&path).expect_err("policy should fail");
        assert!(
            err.to_string().contains("non-empty `profile`"),
            "err={err:#}"
        );
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
