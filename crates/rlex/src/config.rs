use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub paths: PathsConfig,
    pub server: ServerConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PathsConfig {
    /// Root directory for all rlex data (~/.rlex)
    pub root: PathBuf,
    /// Oxigraph store location (~/.rlex/oxigraph)
    pub oxigraph: PathBuf,
    /// Downloaded graph file cache (~/.rlex/cache)
    pub cache: PathBuf,
    /// Cloned repos like forx-index (~/.rlex/repos)
    pub repos: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    /// SPARQL endpoint port
    pub sparql_port: u16,
    /// Viz UI port
    pub viz_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        let root = default_root();
        Config {
            paths: PathsConfig {
                oxigraph: root.join("oxigraph"),
                cache: root.join("cache"),
                repos: root.join("repos"),
                root,
            },
            server: ServerConfig {
                sparql_port: 7878,
                viz_port: 3000,
            },
        }
    }
}

fn default_root() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".rlex")
}

impl Config {
    /// Load config from ~/.rlex/config.toml, creating defaults if missing
    pub fn load() -> Result<Self> {
        let config_path = default_root().join("config.toml");

        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)
                .with_context(|| format!("reading {}", config_path.display()))?;
            let config: Config = toml::from_str(&contents)
                .with_context(|| format!("parsing {}", config_path.display()))?;
            Ok(config)
        } else {
            let config = Config::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Save config to ~/.rlex/config.toml
    pub fn save(&self) -> Result<()> {
        let config_path = self.paths.root.join("config.toml");
        fs::create_dir_all(&self.paths.root)
            .with_context(|| format!("creating {}", self.paths.root.display()))?;

        let contents = toml::to_string_pretty(self)
            .context("serializing config")?;
        fs::write(&config_path, contents)
            .with_context(|| format!("writing {}", config_path.display()))?;

        Ok(())
    }

    /// Ensure all directories exist
    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [&self.paths.root, &self.paths.oxigraph, &self.paths.cache, &self.paths.repos] {
            fs::create_dir_all(dir)
                .with_context(|| format!("creating {}", dir.display()))?;
        }
        Ok(())
    }

    /// Cache path for a specific repo: ~/.rlex/cache/{org}/{repo}
    pub fn cache_path(&self, org: &str, repo: &str) -> PathBuf {
        self.paths.cache.join(org).join(repo)
    }

    /// Repo clone path: ~/.rlex/repos/{org}/{repo}
    pub fn repo_path(&self, org: &str, repo: &str) -> PathBuf {
        self.paths.repos.join(org).join(repo)
    }

    /// Path to the forx-index clone
    pub fn forx_index_path(&self) -> PathBuf {
        self.repo_path("repolex-forx", "forx-index")
    }
}
