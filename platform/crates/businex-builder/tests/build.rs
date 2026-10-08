//! Integration proofs for the isolated app builder.
//!
//! These tests need a working container engine. The builder image is
//! built once on first use (the only step allowed network access); every
//! app build and probe afterwards runs with the network disabled.

use businex_builder::{
    BuildError, DockerIsolation, IsolationConfig, SourceFile, SourceProject,
};
use std::process::Command;
use std::sync::OnceLock;

const FIXTURE_APP: &str = r#"const app: App = {
  async handle(request: businex.Request): Promise<businex.Response> {
    const items = await businex.records.list("item");
    const low: string[] = [];
    for (const item of items) {
      const qty = Number(item.data.qty);
      const min = Number(item.data.min ?? 0);
      if (qty <= min) {
        low.push(String(item.data.sku));
      }
    }
    return { status: 200, body: { lowStock: low, path: request.path } };
  },
};
export default app;
"#;

const BAD_APP: &str = r#"const app: App = {
  async handle(): Promise<businex.Response> {
    return { status: 200 };
  },
};
const wrong: number = "text";
export default app;
"#;

fn ensure_image(iso: &DockerIsolation) {
    static READY: OnceLock<Result<(), String>> = OnceLock::new();
    let result = READY.get_or_init(|| {
        if iso.image_ready().map_err(|e| e.to_string())? {
            return Ok(());
        }
        let context = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("image");
        iso.build_image(&context).map_err(|e| e.to_string())
    });
    if let Err(message) = result {
        panic!("builder image unavailable: {}", message);
    }
}

fn builder(name_prefix: &str) -> DockerIsolation {
    let iso = DockerIsolation::new(IsolationConfig {
        name_prefix: name_prefix.into(),
        ..IsolationConfig::default()
    });
    ensure_image(&iso);
    iso
}

fn project(content: &str) -> SourceProject {
    SourceProject::new(
        vec![SourceFile {
            path: "app.ts".into(),
            content: content.into(),
        }],
        vec![],
    )
    .expect("valid project")
}

#[test]
fn isolated_build_compiles_typechecks_and_executes() {
    let iso = builder("businex-bt-happy");
    let input = serde_json::json!({
        "records": [
            {"entity": "item", "data": {"sku": "A-1", "qty": 1, "min": 5}},
            {"entity": "item", "data": {"sku": "B-2", "qty": 9, "min": 2}}
        ],
        "request": {"method": "GET", "path": "/low-stock", "query": {}}
    });
    let out = iso
        .run_build(&project(FIXTURE_APP), Some(&input))
        .expect("build");
    assert!(!out.artifact_hash.is_empty());
    assert!(
        out.artifact.files.iter().any(|f| f.path == "app.js"),
        "{:?}",
        out.artifact.files
    );
    let run = out.run.expect("run outcome");
    assert!(run.ok, "handler failed: {:?}", run.error);
    assert_eq!(
        run.response["body"]["lowStock"],
        serde_json::json!(["A-1"]),
        "the app's own logic computes low stock: {}",
        run.response
    );
    assert_eq!(run.response["body"]["path"], "/low-stock");

    let again = iso
        .run_build(&project(FIXTURE_APP), Some(&input))
        .expect("second build");
    assert_eq!(
        out.artifact_hash, again.artifact_hash,
        "content addressing is stable across identical builds"
    );
}

#[test]
fn build_reports_type_errors() {
    let iso = builder("businex-bt-types");
    match iso.run_build(&project(BAD_APP), None) {
        Err(BuildError::Compile(diagnostics)) => {
            assert!(
                diagnostics.iter().any(|d| d.code.starts_with("TS")),
                "{:?}",
                diagnostics
            );
            assert!(
                diagnostics.iter().any(|d| d.message.contains("number")),
                "{:?}",
                diagnostics
            );
        }
        other => panic!("expected a compile failure, got {:?}", other),
    }
}

#[test]
fn isolation_profile_blocks_network_and_drops_root() {
    let iso = builder("businex-bt-profile");
    let probe = iso
        .run_isolated(
            &[
                "node",
                "-e",
                "fetch('http://example.com').then(function(){process.exit(1)},function(){process.exit(0)})",
            ],
            60,
        )
        .expect("network probe");
    assert_eq!(
        probe.status, 0,
        "network fetch must fail inside the build: {}",
        probe.stderr
    );
    let user = iso.run_isolated(&["id", "-u"], 60).expect("user probe");
    assert_eq!(user.stdout.trim(), "65534", "builds never run as root");
}

#[test]
fn builds_are_killed_on_timeout_and_containers_cleaned() {
    let iso = builder("businex-bt-timeout");
    let err = iso
        .run_isolated(&["node", "-e", "for(;;){}"], 2)
        .expect_err("a runaway build must time out");
    assert!(matches!(err, BuildError::Timeout(2)), "{:?}", err);
    let ps = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            "name=businex-bt-timeout",
            "--format",
            "{{.Names}}",
        ])
        .output()
        .expect("docker ps");
    assert!(
        String::from_utf8_lossy(&ps.stdout).trim().is_empty(),
        "timed-out containers must be removed: {}",
        String::from_utf8_lossy(&ps.stdout)
    );
}
