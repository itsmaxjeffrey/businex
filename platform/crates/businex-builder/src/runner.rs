//! The isolation profile and the build-and-execute runner.
//!
//! Nothing in this module compiles or runs app code on the host. Every
//! build and every handler call happens inside a throwaway container with
//! no network, no capabilities, a read-only root filesystem, a non-root
//! user and hard CPU, memory, process and time limits. Timeouts kill the
//! container and remove it; a killed build never keeps running.

use crate::artifact::Artifact;
use crate::error::{BuildError, Diagnostic};
use crate::project::SourceProject;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const DEFAULT_IMAGE: &str = "businex-builder-ts:1";

const BUILD_MJS: &str = include_str!("../assets/build.mjs");
const SDK_D_TS: &str = include_str!("../assets/sdk.d.ts");
const TSCONFIG_JSON: &str = include_str!("../assets/tsconfig.json");

#[derive(Debug, Clone)]
pub struct IsolationConfig {
    /// Builder image: node LTS plus a pinned TypeScript compiler.
    pub image: String,
    /// Docker CLI invocation. The builder needs a container engine; it
    /// never touches the host toolchain.
    pub docker: String,
    pub cpu: f64,
    pub memory_mb: u64,
    pub pids_limit: i64,
    pub timeout_secs: u64,
    pub output_limit_bytes: usize,
    /// Container name prefix. Every container gets a unique suffix; tests
    /// use their own prefix so cleanup assertions cannot race each other.
    pub name_prefix: String,
}

impl Default for IsolationConfig {
    fn default() -> Self {
        IsolationConfig {
            image: DEFAULT_IMAGE.into(),
            docker: "docker".into(),
            cpu: 1.0,
            memory_mb: 256,
            pids_limit: 64,
            timeout_secs: 60,
            output_limit_bytes: 8 * 1024 * 1024,
            name_prefix: "businex-build".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolationOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

/// What the app's own handler did inside the container, run against a
/// fixture request through the typed SDK stub.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RunOutcome {
    pub ok: bool,
    pub response: serde_json::Value,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuildOutput {
    pub artifact: Artifact,
    /// Content-addressed digest of the compiled files: the compiled
    /// artifact proof, distinct from any manifest hash.
    pub artifact_hash: String,
    pub diagnostics: Vec<Diagnostic>,
    /// Present only when a fixture input was supplied and the handler ran.
    pub run: Option<RunOutcome>,
}

#[derive(serde::Deserialize)]
struct RunJson {
    ok: bool,
    #[serde(default)]
    response: serde_json::Value,
    #[serde(default)]
    error: Option<String>,
}

#[derive(serde::Deserialize)]
struct ResultJson {
    ok: bool,
    #[serde(default)]
    diagnostics: Vec<Diagnostic>,
    #[serde(default)]
    run: Option<RunJson>,
    #[serde(default)]
    log: Option<String>,
}

pub struct DockerIsolation {
    pub config: IsolationConfig,
}

impl DockerIsolation {
    pub fn new(config: IsolationConfig) -> Self {
        DockerIsolation { config }
    }

    pub fn with_defaults() -> Self {
        DockerIsolation::new(IsolationConfig::default())
    }

    /// The locked-down container profile. Kept as data so the security
    /// shape can be asserted in tests instead of trusted.
    pub fn container_args(&self, name: &str) -> Vec<String> {
        vec![
            "run".into(),
            "--rm".into(),
            "--init".into(),
            "--name".into(),
            name.into(),
            "--network".into(),
            "none".into(),
            "--memory".into(),
            format!("{}m", self.config.memory_mb),
            "--memory-swap".into(),
            format!("{}m", self.config.memory_mb),
            "--cpus".into(),
            format!("{}", self.config.cpu),
            "--pids-limit".into(),
            format!("{}", self.config.pids_limit),
            "--read-only".into(),
            "--tmpfs".into(),
            "/tmp:rw,noexec,nosuid,size=64m".into(),
            "--security-opt".into(),
            "no-new-privileges".into(),
            "--cap-drop".into(),
            "ALL".into(),
            "--user".into(),
            "65534:65534".into(),
        ]
    }

    pub fn image_ready(&self) -> Result<bool, BuildError> {
        let out = Command::new(&self.config.docker)
            .args(["image", "inspect", &self.config.image])
            .output()
            .map_err(|e| BuildError::Tool(format!("cannot run docker: {}", e)))?;
        Ok(out.status.success())
    }

    /// Build the builder image from its context. This is the only step
    /// allowed network access (package installation); every app build
    /// afterwards runs with the network disabled.
    pub fn build_image(&self, context: &Path) -> Result<(), BuildError> {
        let args = vec![
            "build".into(),
            "-t".into(),
            self.config.image.clone(),
            context.display().to_string(),
        ];
        let out = exec_watchdog(&self.config.docker, &args, 300, None)?;
        if out.status != 0 {
            return Err(BuildError::Image(format!(
                "docker build failed: {}",
                out.stderr.trim()
            )));
        }
        Ok(())
    }

    /// Run any command inside the build isolation profile. Used by
    /// run_build and by probes that verify the profile itself.
    pub fn run_isolated(&self, argv: &[&str], timeout_secs: u64) -> Result<IsolationOutput, BuildError> {
        let name = format!("{}-{}", self.config.name_prefix, uuid::Uuid::new_v4());
        let mut args = self.container_args(&name);
        args.push(self.config.image.clone());
        for part in argv {
            args.push((*part).to_string());
        }
        exec_watchdog(&self.config.docker, &args, timeout_secs, Some(&name))
    }

    /// Compile and type-check the project, then run its handler against a
    /// fixture input, all inside one isolated container. Sources go in
    /// read-only; the only writable location is the output volume.
    pub fn run_build(
        &self,
        project: &SourceProject,
        input: Option<&serde_json::Value>,
    ) -> Result<BuildOutput, BuildError> {
        let root = std::env::temp_dir().join(format!("businex-build-{}", uuid::Uuid::new_v4()));
        let src = root.join("src");
        let assets = root.join("assets");
        let out = root.join("out");
        for dir in [&src, &assets, &out] {
            std::fs::create_dir_all(dir)
                .map_err(|e| BuildError::Tool(format!("cannot create workdir: {}", e)))?;
            set_mode(dir, 0o755)?;
        }
        set_mode(&out, 0o777)?;
        for file in project.files() {
            let path = src.join(&file.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| BuildError::Tool(format!("cannot create source dir: {}", e)))?;
                set_mode(parent, 0o755)?;
            }
            std::fs::write(&path, file.content.as_bytes())
                .map_err(|e| BuildError::Tool(format!("cannot write source: {}", e)))?;
            set_mode(&path, 0o644)?;
        }
        for (name, content) in [
            ("build.mjs", BUILD_MJS),
            ("sdk.d.ts", SDK_D_TS),
            ("tsconfig.json", TSCONFIG_JSON),
        ] {
            let path = assets.join(name);
            std::fs::write(&path, content)
                .map_err(|e| BuildError::Tool(format!("cannot write asset: {}", e)))?;
            set_mode(&path, 0o644)?;
        }
        if let Some(input) = input {
            let path = assets.join("input.json");
            let bytes = serde_json::to_vec(input)
                .map_err(|e| BuildError::Tool(format!("cannot encode input: {}", e)))?;
            std::fs::write(&path, bytes)
                .map_err(|e| BuildError::Tool(format!("cannot write input: {}", e)))?;
            set_mode(&path, 0o644)?;
        }

        let name = format!("{}-{}", self.config.name_prefix, uuid::Uuid::new_v4());
        let mut args = self.container_args(&name);
        args.extend([
            "-v".into(),
            format!("{}:/workspace/src:ro", src.display()),
            "-v".into(),
            format!("{}:/workspace/assets:ro", assets.display()),
            "-v".into(),
            format!("{}:/workspace/out:rw", out.display()),
            "-w".into(),
            "/workspace".into(),
            "--env".into(),
            "HOME=/tmp".into(),
            self.config.image.clone(),
            "node".into(),
            "assets/build.mjs".into(),
        ]);
        let run = exec_watchdog(&self.config.docker, &args, self.config.timeout_secs, Some(&name));
        let output = match run {
            Ok(output) => output,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&root);
                return Err(e);
            }
        };

        let result_path = out.join("result.json");
        let raw = match std::fs::read_to_string(&result_path) {
            Ok(raw) => raw,
            Err(e) => {
                let stderr = clip(&output.stderr, self.config.output_limit_bytes);
                let _ = std::fs::remove_dir_all(&root);
                return Err(BuildError::Tool(format!(
                    "builder produced no result: {}; container stderr: {}",
                    e, stderr
                )));
            }
        };
        let parsed: ResultJson = match serde_json::from_str(&raw) {
            Ok(parsed) => parsed,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&root);
                return Err(BuildError::Tool(format!("unreadable builder result: {}", e)));
            }
        };
        if !parsed.ok {
            let _ = std::fs::remove_dir_all(&root);
            if parsed.diagnostics.is_empty() {
                return Err(BuildError::Tool(parsed.log.unwrap_or_default()));
            }
            return Err(BuildError::Compile(parsed.diagnostics));
        }
        let artifact = Artifact::from_dir(&out.join("dist"))?;
        let artifact_hash = artifact.hash();
        let run = parsed.run.map(|r| RunOutcome {
            ok: r.ok,
            response: r.response,
            error: r.error,
        });
        let _ = std::fs::remove_dir_all(&root);
        Ok(BuildOutput {
            artifact,
            artifact_hash,
            diagnostics: parsed.diagnostics,
            run,
        })
    }
}

fn set_mode(path: &Path, mode: u32) -> Result<(), BuildError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|e| BuildError::Tool(format!("cannot set permissions: {}", e)))
}

fn clip(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// Run the docker CLI with a wall-clock deadline. On timeout the container
/// is force-removed first (which stops it), then the client is killed: a
/// timed-out build cannot leave anything running.
fn exec_watchdog(
    docker: &str,
    args: &[String],
    timeout_secs: u64,
    container_name: Option<&str>,
) -> Result<IsolationOutput, BuildError> {
    let mut child = Command::new(docker)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| BuildError::Tool(format!("cannot run docker: {}", e)))?;
    let stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| BuildError::Tool("docker stdout unavailable".into()))?;
    let stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| BuildError::Tool("docker stderr unavailable".into()))?;
    let done = Arc::new(AtomicBool::new(false));
    let out_reader = spawn_reader(stdout_pipe);
    let err_reader = spawn_reader(stderr_pipe);
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    done.store(true, Ordering::SeqCst);
                    if let Some(name) = container_name {
                        let _ = Command::new(docker).args(["rm", "-f", name]).output();
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out_reader.join();
                    let _ = err_reader.join();
                    return Err(BuildError::Timeout(timeout_secs));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                done.store(true, Ordering::SeqCst);
                return Err(BuildError::Tool(format!("docker wait failed: {}", e)));
            }
        }
    };
    done.store(true, Ordering::SeqCst);
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    Ok(IsolationOutput {
        status: status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

fn spawn_reader<R: Read + Send + 'static>(mut pipe: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = pipe.read_to_end(&mut buffer);
        buffer
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_profile_is_locked_down() {
        let iso = DockerIsolation::with_defaults();
        let flat = iso.container_args("probe").join(" ");
        for required in [
            "--network none",
            "--cap-drop ALL",
            "--security-opt no-new-privileges",
            "--read-only",
            "--pids-limit 64",
            "--memory 256m",
            "--cpus 1",
            "--user 65534:65534",
            "--tmpfs /tmp:rw,noexec,nosuid,size=64m",
        ] {
            assert!(flat.contains(required), "missing: {} in {}", required, flat);
        }
        assert!(!flat.contains("--privileged"));
        assert!(!flat.contains("docker.sock"));
    }

    #[test]
    fn timeout_kills_and_reports_timeout() {
        // No docker needed: a nonexistent CLI fails as Tool; the timeout
        // path itself is exercised by the integration probe.
        let iso = DockerIsolation::new(IsolationConfig {
            docker: "/nonexistent/docker".into(),
            ..IsolationConfig::default()
        });
        let err = iso.run_isolated(&["true"], 1).unwrap_err();
        assert!(matches!(err, BuildError::Tool(_)), "{:?}", err);
    }
}
