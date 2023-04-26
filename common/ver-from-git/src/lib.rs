#![deny(warnings)]

use anyhow::{ensure, Result};
use git2::{ObjectType, Oid, Repository};
use semver::Version;
use std::{collections::HashMap, env};
/// Retrieves version information from current crate's Git repository and pushes it to Cargo's build config
/// as env variable `DEX_CORE_VERSION`. If no Git repo is found, version is `0.0.0+unknown`
pub fn version_from_git() {
    let ver_str = match read_ver_tag() {
        Ok(s) => s,
        Err(e) => {
            println!("cargo:warning=Failed to retrieve version info from Git repository. Will use default version stub. Error: {e}");
            "0.0.0+unknown".to_string()
        }
    };

    println!("cargo:rustc-env=DEX_CORE_VERSION={ver_str}");
}

const VERSION_PREFIX: &str = "v";
const TAG_PREFIX: &str = "refs/tags/";

fn read_ver_tag() -> Result<String> {
    fn try_run(cb: impl FnOnce() -> Result<()>) -> bool {
        std::mem::drop(cb());
        true
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR")?;
    let repo = Repository::discover(manifest_dir)?;
    // Read all tags which match `<semver>` or `v<semver>` pattern, store them
    let mut tags: HashMap<Oid, Vec<Version>> = HashMap::new();

    repo.tag_foreach(|oid, name| {
        try_run(|| {
            let name = std::str::from_utf8(name)?;
            let name = name.strip_prefix(TAG_PREFIX).unwrap_or(name);
            // Parse tag as SEMVER. Accept both normal "<semver>" and "v<semver>".
            let ver: Version = name.strip_prefix(VERSION_PREFIX).unwrap_or(name).parse()?;
            // Skip tags which have "build" section
            ensure!(
                ver.build.is_empty(),
                "Tags with non-empty build sufffix are skipped"
            );
            // Fetch object and determine its target based on its type
            let target_obj = repo.find_object(oid, None)?;
            let target_id = match target_obj.kind() {
                // Just another name for commit
                Some(ObjectType::Commit) => oid,
                // Distinct tag object which points to commit
                Some(ObjectType::Tag) => target_obj.into_tag().unwrap().target_id(), // Shouldn't fail
                Some(kind) => anyhow::bail!("Incorrect object type: {kind:?}"),
                None => anyhow::bail!("Unknown object type"),
            };
            // Store using target Oid as key
            tags.entry(target_id).or_default().push(ver);
            Ok(())
        })
    })?;
    // Sort versions for each target in descending order, to make sure highest one is always picked
    for vers in tags.values_mut() {
        vers.sort_by(|l, r| r.cmp(l));
    }
    // Get head and walk first-parent path until we find any
    let mut commit = repo.head()?.peel_to_commit()?;
    let head_id = commit.id();

    loop {
        if let Some(vers) = tags.get_mut(&commit.id()) {
            let mut ver = vers.first_mut().unwrap().clone(); // Panicking here is a definite bug
            ver.build = head_id.to_string().parse().unwrap(); // Panicking here is a definite bug too

            return Ok(ver.to_string());
        }
        // Next commit, if any. Not finding version up to repo root is also a failure
        commit = commit.parent(0)?;
    }
}
