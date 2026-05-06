//! `xtask` — developer tooling for the atomr-view workspace.
//!
//! Subcommands:
//! * `bump` — bump the workspace version (and `pyproject.toml`),
//!   refresh `Cargo.lock`. Drives the `version-bump` Claude skill
//!   and `.github/workflows/version-bump.yml`.
//! * `verify` — `cargo build` + `cargo test` + `cargo clippy` (the
//!   gate that `release.yml` runs before producing artifacts).

use std::env;
use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, Context, Result};

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "help".into());
    match cmd.as_str() {
        "bump" => bump(args.collect()),
        "verify" => verify(),
        "help" | "-h" | "--help" => {
            print_help();
            Ok(())
        }
        other => Err(anyhow!("unknown xtask subcommand: {other}")),
    }
}

fn print_help() {
    println!("atomr-view xtask");
    println!();
    println!("USAGE:");
    println!("  cargo xtask <subcommand>");
    println!();
    println!("SUBCOMMANDS:");
    println!("  bump <patch|minor|major|--pre <id>|--set <ver>>");
    println!("                       bump workspace + pyproject version, refresh Cargo.lock");
    println!("  verify               run build + test + clippy (release gate)");
    println!("  help                 print this help");
}

fn bump(args: Vec<String>) -> Result<()> {
    let mut iter = args.into_iter();
    let arg = iter
        .next()
        .ok_or_else(|| anyhow!("usage: bump <patch|minor|major> | bump --pre <id> | bump --set <version>"))?;
    let cargo_toml = Path::new("Cargo.toml");
    let pyproject = Path::new("pyproject.toml");
    let current = read_workspace_version(cargo_toml)?;
    let next = match arg.as_str() {
        "patch" => semver_bump(&current, BumpKind::Patch)?,
        "minor" => semver_bump(&current, BumpKind::Minor)?,
        "major" => semver_bump(&current, BumpKind::Major)?,
        "--pre" => {
            let id = iter.next().ok_or_else(|| anyhow!("--pre requires <id>"))?;
            semver_bump(&current, BumpKind::Pre(id))?
        }
        "--set" => iter.next().ok_or_else(|| anyhow!("--set requires <version>"))?,
        other => return Err(anyhow!("unknown bump arg: {other}")),
    };
    println!("{} -> {}", current, next);
    write_workspace_version(cargo_toml, &next)?;
    write_workspace_deps_versions(cargo_toml, &current, &next)?;
    if pyproject.exists() {
        write_pyproject_version(pyproject, &next)?;
    }
    let _ = Command::new(env!("CARGO")).args(["update", "--workspace"]).status();
    println!("ATOMR_VIEW_NEW_VERSION={next}");
    Ok(())
}

#[derive(Debug)]
enum BumpKind {
    Patch,
    Minor,
    Major,
    Pre(String),
}

fn semver_bump(current: &str, kind: BumpKind) -> Result<String> {
    let (core, _pre) = match current.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (current, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return Err(anyhow!("version `{current}` is not MAJOR.MINOR.PATCH"));
    }
    let mut major: u64 = parts[0].parse().context("major")?;
    let mut minor: u64 = parts[1].parse().context("minor")?;
    let mut patch: u64 = parts[2].parse().context("patch")?;
    let next = match kind {
        BumpKind::Patch => {
            patch += 1;
            format!("{major}.{minor}.{patch}")
        }
        BumpKind::Minor => {
            minor += 1;
            patch = 0;
            format!("{major}.{minor}.{patch}")
        }
        BumpKind::Major => {
            major += 1;
            minor = 0;
            patch = 0;
            format!("{major}.{minor}.{patch}")
        }
        BumpKind::Pre(id) => format!("{major}.{minor}.{patch}-{id}"),
    };
    Ok(next)
}

fn read_workspace_version(path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(path)?;
    let block_start = text
        .find("[workspace.package]")
        .ok_or_else(|| anyhow!("no [workspace.package] block in {}", path.display()))?;
    let block_end = text[block_start..].find("\n[").map(|i| block_start + i).unwrap_or(text.len());
    let block = &text[block_start..block_end];
    for line in block.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("version") {
            let after_eq = rest.split_once('=').map(|(_, v)| v.trim()).unwrap_or("");
            let value = after_eq.trim_matches('"').trim_matches('\'');
            return Ok(value.to_string());
        }
    }
    Err(anyhow!("no version key in [workspace.package]"))
}

fn write_workspace_version(path: &Path, version: &str) -> Result<()> {
    let text = std::fs::read_to_string(path)?;
    let block_start =
        text.find("[workspace.package]").ok_or_else(|| anyhow!("no [workspace.package] block"))?;
    let after_block = &text[block_start..];
    let local_idx = after_block.find("version = ").ok_or_else(|| anyhow!("no version line"))?;
    let abs = block_start + local_idx;
    let line_end = text[abs..].find('\n').map(|i| abs + i).unwrap_or(text.len());
    let new_line = format!("version = \"{version}\"");
    let mut out = String::with_capacity(text.len() + new_line.len());
    out.push_str(&text[..abs]);
    out.push_str(&new_line);
    out.push_str(&text[line_end..]);
    std::fs::write(path, out)?;
    Ok(())
}

/// Bumps the `version = "<prev>"` pin on every internal path-dep line
/// inside `[workspace.dependencies]`. atomr-view currently uses
/// `path = "../X"` directly inside each crate's Cargo.toml rather than
/// declaring path-deps at the workspace level, so this is a no-op
/// today — kept here so a future workspace.dependencies migration
/// stays in lockstep with the workspace version.
fn write_workspace_deps_versions(path: &Path, prev: &str, next: &str) -> Result<()> {
    let text = std::fs::read_to_string(path)?;
    let block_start = match text.find("[workspace.dependencies]") {
        Some(i) => i,
        None => return Ok(()),
    };
    let after = &text[block_start + "[workspace.dependencies]".len()..];
    let block_len = after.find("\n[").map(|i| i + 1).unwrap_or(after.len());
    let head = &text[..block_start];
    let block = &text[block_start..block_start + "[workspace.dependencies]".len() + block_len];
    let tail = &text[block_start + "[workspace.dependencies]".len() + block_len..];

    let needle = format!("version = \"{prev}\"");
    let replacement = format!("version = \"{next}\"");
    let mut new_block = String::with_capacity(block.len());
    for line in block.split_inclusive('\n') {
        if line.contains("path = \"crates/") && line.contains(&needle) {
            new_block.push_str(&line.replace(&needle, &replacement));
        } else {
            new_block.push_str(line);
        }
    }
    let mut out = String::with_capacity(text.len());
    out.push_str(head);
    out.push_str(&new_block);
    out.push_str(tail);
    std::fs::write(path, out)?;
    Ok(())
}

fn write_pyproject_version(path: &Path, version: &str) -> Result<()> {
    let text = std::fs::read_to_string(path)?;
    let mut replaced = false;
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim_start();
        if !replaced && trimmed.starts_with("version") && trimmed.contains('=') {
            out.push_str(&format!("version = \"{version}\"\n"));
            replaced = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !replaced {
        return Err(anyhow!("no version line in pyproject.toml"));
    }
    std::fs::write(path, out)?;
    Ok(())
}

fn verify() -> Result<()> {
    let cargo = env!("CARGO");
    let steps: Vec<(&str, &[&str])> = vec![
        ("cargo build --workspace", &["build", "--workspace"]),
        ("cargo test --workspace --quiet", &["test", "--workspace", "--quiet"]),
        (
            "cargo clippy --workspace --all-targets -- -D warnings",
            &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
        ),
    ];
    for (label, args) in &steps {
        println!("==> {label}");
        let status =
            Command::new(cargo).args(args.iter()).status().with_context(|| format!("spawning `{label}`"))?;
        if !status.success() {
            return Err(anyhow!("{label} failed: {status}"));
        }
    }
    println!("\nverify: OK");
    Ok(())
}
