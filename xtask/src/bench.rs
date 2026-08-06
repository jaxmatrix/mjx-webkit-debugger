//! Run the performance budget benches.
//!
//! Discovers every `[[bench]]` target in the workspace (including peer-owned
//! ones such as T-005's `text` when present), asserts that every required
//! CLAUDE.md budget has an enforcing bench on disk, and runs them so a
//! regression fails the build rather than merely reporting.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use mjx_wk_perf::{Budget, FUTURE_BENCHES};
use serde::Deserialize;

/// Discover, verify, and run every budget bench.
pub fn run(root: &Path) -> Result<()> {
    let metadata = cargo_metadata(root)?;
    let discovered = discover_benches(&metadata);

    println!("perf benches — required budgets");
    let mut missing_required = Vec::new();
    for budget in Budget::all() {
        let target = budget.bench_target();
        let on_disk = root.join(target.path).is_file();
        let in_metadata = discovered
            .iter()
            .any(|d| d.package == target.package && d.bench == target.bench);
        let status = match (on_disk, in_metadata) {
            (true, true) => "ok",
            (true, false) => "on disk, not in cargo metadata",
            (false, _) => {
                if target.required {
                    missing_required.push(format!("{} ({})", budget.name(), target.path));
                }
                "MISSING"
            }
        };
        println!(
            "  [{status}] {budget} → {}/{}",
            target.package, target.bench
        );
    }

    if !missing_required.is_empty() {
        bail!(
            "required budget benches are missing:\n  {}",
            missing_required.join("\n  ")
        );
    }

    println!("perf benches — peer-owned (run when present)");
    for future in FUTURE_BENCHES {
        let on_disk = root.join(future.path).is_file();
        let in_metadata = discovered
            .iter()
            .any(|d| d.package == future.package && d.bench == future.bench);
        if on_disk && in_metadata {
            println!(
                "  [ok] {}/{} (owner {})",
                future.package, future.bench, future.owner
            );
        } else {
            println!(
                "  [pending {}] {} — not present yet",
                future.owner, future.path
            );
        }
    }

    // Required budgets first, then any other discovered benches (e.g. text).
    let mut to_run: Vec<(String, String)> = Budget::all()
        .into_iter()
        .map(|b| {
            let t = b.bench_target();
            (t.package.to_owned(), t.bench.to_owned())
        })
        .collect();

    for d in &discovered {
        if !to_run
            .iter()
            .any(|(pkg, bench)| pkg == &d.package && bench == &d.bench)
        {
            to_run.push((d.package.clone(), d.bench.clone()));
        }
    }

    println!("running {} bench target(s)", to_run.len());
    let mut failed = Vec::new();
    for (package, bench) in &to_run {
        println!("── cargo test -p {package} --bench {bench}");
        // Assertion benches (harness = false) are run via `cargo test --bench`
        // so they share the debug/test artifact graph. `cargo bench` would
        // rebuild under the bench profile and thrash CI for no timing gain.
        let status = Command::new(env!("CARGO"))
            .current_dir(root)
            .args(["test", "-p", package, "--bench", bench, "--", "--nocapture"])
            .status()
            .with_context(|| format!("spawning cargo test --bench for {package}/{bench}"))?;
        if !status.success() {
            failed.push(format!("{package}/{bench}"));
        }
    }

    if !failed.is_empty() {
        bail!(
            "perf budget bench(es) failed: {}\n\
             thresholds live in crates/mjx-wk-perf — raise them there only with a measured reason",
            failed.join(", ")
        );
    }

    println!("all perf budget benches passed");
    Ok(())
}

#[derive(Debug, Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    id: String,
    targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
struct Target {
    name: String,
    kind: Vec<String>,
}

#[derive(Debug, Clone)]
struct DiscoveredBench {
    package: String,
    bench: String,
}

fn cargo_metadata(root: &Path) -> Result<Metadata> {
    let output = Command::new(env!("CARGO"))
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .context("running `cargo metadata`")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout).context("parsing cargo metadata JSON")
}

fn discover_benches(metadata: &Metadata) -> Vec<DiscoveredBench> {
    let members: HashSet<&str> = metadata
        .workspace_members
        .iter()
        .map(String::as_str)
        .collect();

    let mut out = Vec::new();
    for pkg in &metadata.packages {
        if !members.contains(pkg.id.as_str()) {
            continue;
        }
        for target in &pkg.targets {
            if target.kind.iter().any(|k| k == "bench") {
                out.push(DiscoveredBench {
                    package: pkg.name.clone(),
                    bench: target.name.clone(),
                });
            }
        }
    }
    out.sort_by(|a, b| (&a.package, &a.bench).cmp(&(&b.package, &b.bench)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_text_bench_is_optional() {
        let text = FUTURE_BENCHES
            .iter()
            .find(|f| f.bench == "text")
            .expect("T-005 text bench is registered as future");
        assert_eq!(text.owner, "T-005");
        assert!(text.path.ends_with("benches/text.rs"));
    }

    #[test]
    fn required_budgets_cover_claude_table() {
        let names: Vec<_> = Budget::all().iter().map(|b| b.name()).collect();
        assert!(names.iter().any(|n| n.contains("attach")));
        assert!(names.iter().any(|n| n.contains("60 fps")));
        assert!(names.iter().any(|n| n.contains("pause")));
        assert!(names.iter().any(|n| n.contains("16 ms")));
    }
}
