//! Generates `mjx-wk-protocol`'s domain modules from WebKit's protocol
//! descriptions.
//!
//! Input is `reference/webkit-protocol/*.json` (git-ignored, see that
//! directory's README). Output is `crates/mjx-wk-protocol/src/generated/`,
//! which **is committed** — a clean clone builds with no input present, and
//! regenerating is a reviewable diff rather than a build-time surprise.
//!
//! # Emission layout
//!
//! One module per domain. Types live at the module root because that is how
//! cross-domain `$ref`s address them (`Debugger.Location` →
//! `generated::debugger::Location`); commands and events get their own
//! namespaces, which is what keeps `Animation`'s `TrackingUpdate` type from
//! colliding with its `trackingUpdate` event.
//!
//! ```text
//! generated::debugger::Location            // a type
//! generated::debugger::commands::Resume    // a command
//! generated::debugger::events::Paused      // an event
//! ```

pub mod emit;
pub mod merge;
pub mod naming;
pub mod schema;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use schema::DomainFile;

/// Where the protocol descriptions are read from, relative to the repo root.
pub const INPUT_DIR: &str = "reference/webkit-protocol";
/// Where the generated Rust is written, relative to the repo root.
pub const OUTPUT_DIR: &str = "crates/mjx-wk-protocol/src/generated";

/// The pseudo-domain holding types shared across real domains. It gets a
/// module but is not a `Domain` variant.
pub const PSEUDO_DOMAIN: &str = "GenericTypes";

/// The WebKit tag `reference/webkit-protocol/` is pinned to.
///
/// Kept in step with `reference/README.md`. Recorded into the generated
/// manifest so `verify-protocol` can say what the output was built from.
pub const PINNED_REF: &str = "webkitgtk-2.52.3";

/// Read every domain description, in a stable order.
pub fn load(root: &Path) -> Result<Vec<DomainFile>> {
    let dir = root.join(INPUT_DIR);
    if !dir.is_dir() {
        bail!(
            "{} is missing.\n\
             It is git-ignored local-only material; see reference/README.md for the \
             one-liner that repopulates it. The committed output in {} lets a clean \
             clone build without it — you only need it to regenerate.",
            dir.display(),
            OUTPUT_DIR
        );
    }

    let mut files = Vec::new();
    let mut paths: Vec<_> = fs::read_dir(&dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();

    for path in paths {
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let parsed: DomainFile =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        files.push(parsed);
    }

    if files.is_empty() {
        bail!("{} contains no .json files", dir.display());
    }
    Ok(files)
}

/// Generate every domain module plus the `mod.rs` that ties them together.
pub fn run(root: &Path) -> Result<()> {
    let files = load(root)?;
    let out = root.join(OUTPUT_DIR);
    fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;

    // Which type ids each domain declares, so a `$ref` can be resolved to a
    // real path and a dangling one reported rather than emitted as broken Rust.
    let index: BTreeMap<String, Vec<String>> = files
        .iter()
        .map(|f| {
            (
                f.domain.clone(),
                f.types.iter().map(|t| t.id.clone()).collect(),
            )
        })
        .collect();

    // Remove modules for domains that no longer exist upstream, so a stale file
    // cannot keep compiling after the protocol drops a domain.
    for entry in fs::read_dir(&out).with_context(|| format!("reading {}", out.display()))? {
        let path = entry?.path();
        let keep = path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|stem| {
                stem == "mod"
                    || files
                        .iter()
                        .any(|f| naming::module_ident(&f.domain) == stem)
            });
        if !keep && path.extension().is_some_and(|e| e == "rs") {
            fs::remove_file(&path)?;
        }
    }

    // Merge every domain before emitting any: recursion can run between
    // domains, so the cycle analysis needs the whole graph.
    let merged: Vec<merge::MergedDomain> =
        files.iter().map(merge::domain).collect::<Result<_>>()?;
    let boxed = emit::analyse_cycles(&merged);
    if !boxed.is_empty() {
        println!("boxing {} self-referential field(s):", boxed.len());
        for (ty, field) in &boxed {
            println!("  - {ty}.{field}");
        }
    }

    let mut modules = Vec::new();
    let mut manifest = crate::verify::ProtocolManifest {
        pinned_ref: PINNED_REF.to_owned(),
        ..Default::default()
    };

    for (file, merged) in files.iter().zip(&merged) {
        let module = naming::module_ident(&file.domain);
        let source = emit::domain(merged, &index, &boxed)?;
        let path = out.join(format!("{module}.rs"));
        fs::write(&path, source).with_context(|| format!("writing {}", path.display()))?;

        // `GenericTypes` is a bag of shared types, not a domain: it has no
        // members and no `Domain` variant, so it is not part of the manifest
        // that `verify-protocol` diffs against the running WebKit.
        if file.domain != PSEUDO_DOMAIN {
            manifest.domains.insert(
                file.domain.clone(),
                crate::verify::DomainMembers {
                    commands: merged.commands.iter().map(|c| c.name.clone()).collect(),
                    events: merged.events.iter().map(|e| e.name.clone()).collect(),
                },
            );
        }
        modules.push((module, file.domain.clone(), merged.description.clone()));
    }

    fs::write(out.join("mod.rs"), emit::mod_rs(&modules))?;

    // Committed alongside the generated code so `verify-protocol` works on a
    // clean clone, where `reference/` is absent.
    let manifest_path = root.join(crate::verify::MANIFEST);
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest)? + "\n",
    )
    .with_context(|| format!("writing {}", manifest_path.display()))?;

    println!(
        "generated {} domain modules into {}\nwrote {}",
        modules.len(),
        out.display(),
        manifest_path.display()
    );
    Ok(())
}
