//! Multi-store TOML configuration. Each `[[stores]]` entry is fully
//! self-contained: a provider plus that provider's settings inline, using
//! the documented env-var names lowercased (aws_bucket, local_path, …).
//! Values may reference the ambient environment with `${VAR}` so secrets
//! stay out of the file, and any key not present falls through to the
//! ambient env entirely.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;
use slatedb::object_store::ObjectStore;

#[derive(Deserialize, Debug)]
pub struct ConfigFile {
    pub stores: Vec<StoreConfig>,
}

#[derive(Deserialize, Debug)]
pub struct StoreConfig {
    /// Unique store name; becomes the prefix of every DB id ("name:path").
    pub name: String,
    /// local | memory | aws | azure
    pub provider: String,
    /// Prefixes to scan for DBs; default: the store root.
    #[serde(default = "default_roots")]
    pub roots: Vec<String>,
    /// Provider settings, keyed by the documented env-var names lowercased.
    #[serde(flatten)]
    pub vars: HashMap<String, toml::Value>,
}

fn default_roots() -> Vec<String> {
    vec![String::new()]
}

pub struct BuiltStore {
    pub name: String,
    pub provider: String,
    pub object_store: Arc<dyn ObjectStore>,
    pub roots: Vec<String>,
}

pub fn parse(text: &str) -> anyhow::Result<ConfigFile> {
    let cfg: ConfigFile = toml::from_str(text)?;
    if cfg.stores.is_empty() {
        anyhow::bail!("config has no [[stores]] entries");
    }
    for s in &cfg.stores {
        if s.name.is_empty()
            || !s
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            anyhow::bail!(
                "store name '{}' is invalid (use [A-Za-z0-9_-]+; ':' is the id separator)",
                s.name
            );
        }
    }
    let mut names: Vec<&str> = cfg.stores.iter().map(|s| s.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    if names.len() != cfg.stores.len() {
        anyhow::bail!("store names must be unique");
    }
    Ok(cfg)
}

/// Expands `${VAR}` against the ambient environment. Unset vars are a hard
/// error — silently empty credentials are worse than failing to start.
fn expand_vars(input: &str, ctx: &str) -> anyhow::Result<String> {
    let mut out = String::new();
    let mut rest = input;
    while let Some(i) = rest.find("${") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let Some(j) = after.find('}') else {
            anyhow::bail!("{ctx}: unclosed ${{ in value");
        };
        let var = &after[..j];
        let val = std::env::var(var)
            .map_err(|_| anyhow::anyhow!("{ctx}: environment variable '{var}' is not set"))?;
        out.push_str(&val);
        rest = &after[j + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn value_to_string(v: &toml::Value, ctx: &str) -> anyhow::Result<String> {
    match v {
        toml::Value::String(s) => Ok(s.clone()),
        toml::Value::Integer(i) => Ok(i.to_string()),
        toml::Value::Float(f) => Ok(f.to_string()),
        toml::Value::Boolean(b) => Ok(b.to_string()),
        other => anyhow::bail!("{ctx}: unsupported value type {}", other.type_str()),
    }
}

/// Builds the object store for one config entry by staging its settings as
/// env vars and calling slatedb's per-provider loader, then restoring the
/// previous environment. slatedb's loaders only read the process env, so
/// this MUST run before the async runtime spawns threads (env mutation is
/// only sound while single-threaded); main() builds all stores before
/// starting tokio. Keys not set here fall through to the ambient env.
pub fn build_store(cfg: &StoreConfig) -> anyhow::Result<Arc<dyn ObjectStore>> {
    let mut staged: Vec<(String, String)> = Vec::new();
    for (key, value) in &cfg.vars {
        let ctx = format!("store '{}', key '{}'", cfg.name, key);
        let value = expand_vars(&value_to_string(value, &ctx)?, &ctx)?;
        staged.push((key.to_uppercase(), value));
    }

    let mut saved: Vec<(String, Option<String>)> = Vec::new();
    for (key, value) in &staged {
        saved.push((key.clone(), std::env::var(key).ok()));
        std::env::set_var(key, value);
    }

    let result = match cfg.provider.to_lowercase().as_str() {
        "local" => slatedb::admin::load_local(),
        "memory" => slatedb::admin::load_memory(),
        "aws" => slatedb::admin::load_aws(),
        "azure" => slatedb::admin::load_azure(),
        other => {
            // Restore before bailing.
            restore(&saved);
            anyhow::bail!(
                "store '{}': unsupported provider '{other}' (local | memory | aws | azure)",
                cfg.name
            );
        }
    };

    restore(&saved);
    result.map_err(|e| anyhow::anyhow!("store '{}': {e}", cfg.name))
}

fn restore(saved: &[(String, Option<String>)]) {
    for (key, old) in saved {
        match old {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
}

/// All stores from a config file, built sequentially pre-runtime.
pub fn build_stores(text: &str) -> anyhow::Result<Vec<BuiltStore>> {
    let cfg = parse(text)?;
    cfg.stores
        .iter()
        .map(|s| {
            Ok(BuiltStore {
                name: s.name.clone(),
                provider: s.provider.to_lowercase(),
                object_store: build_store(s)?,
                roots: s.roots.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inline_store_config() {
        let cfg = parse(
            r#"
            [[stores]]
            name = "local"
            provider = "local"
            local_path = "/tmp/x"
            roots = ["", "teams/"]
            "#,
        )
        .unwrap();
        assert_eq!(cfg.stores.len(), 1);
        assert_eq!(cfg.stores[0].roots, vec!["", "teams/"]);
        assert_eq!(
            cfg.stores[0].vars.get("local_path").unwrap().as_str(),
            Some("/tmp/x")
        );
    }

    #[test]
    fn rejects_bad_names_and_duplicates() {
        assert!(parse("[[stores]]\nname = \"a:b\"\nprovider = \"local\"").is_err());
        assert!(parse(
            "[[stores]]\nname = \"a\"\nprovider = \"local\"\n[[stores]]\nname = \"a\"\nprovider = \"local\""
        )
        .is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn expands_env_references() {
        std::env::set_var("DASH_TEST_VAR", "sekrit");
        assert_eq!(
            expand_vars("a-${DASH_TEST_VAR}-b", "test").unwrap(),
            "a-sekrit-b"
        );
        let err = expand_vars("${DASH_TEST_MISSING}", "store 'x', key 'k'")
            .unwrap_err()
            .to_string();
        assert!(err.contains("DASH_TEST_MISSING"), "{err}");
        assert!(expand_vars("${oops", "test").is_err());
    }

    #[test]
    fn builds_local_store_and_restores_env() {
        let dir = std::env::temp_dir().join("sdb-dash-config-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("LOCAL_PATH", "/nonexistent-ambient");
        let cfg = StoreConfig {
            name: "t".into(),
            provider: "local".into(),
            roots: vec![String::new()],
            vars: HashMap::from([(
                "local_path".to_string(),
                toml::Value::String(dir.to_string_lossy().into_owned()),
            )]),
        };
        build_store(&cfg).unwrap();
        // The ambient value is back after the build.
        assert_eq!(
            std::env::var("LOCAL_PATH").unwrap(),
            "/nonexistent-ambient"
        );
        std::env::remove_var("LOCAL_PATH");
    }
}
