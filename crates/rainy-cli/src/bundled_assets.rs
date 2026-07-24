use crate::error::{RainyError, RainyResult};
use fs2::FileExt;
use include_dir::{Dir, include_dir};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

static SCHEMAS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../schemas");

pub fn registry_path() -> RainyResult<PathBuf> {
    crate::defaults::packs_path()
}

pub fn schema_path() -> RainyResult<PathBuf> {
    source_or_embedded("schemas")
}

pub fn skills_path() -> RainyResult<PathBuf> {
    crate::defaults::skills_path()
}

fn source_or_embedded(name: &str) -> RainyResult<PathBuf> {
    let source = workspace_root().join(name);
    if std::env::var_os("RAINY_FORCE_EMBEDDED_ASSETS").is_none() && source.is_dir() {
        return Ok(source);
    }
    Ok(extract_embedded_schemas()?.join(name))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .to_path_buf()
}

fn extract_embedded_schemas() -> RainyResult<PathBuf> {
    let cache = std::env::var_os("RAINY_ASSET_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let root = cache.join(format!("rainy-cli-schemas-{}", env!("CARGO_PKG_VERSION")));
    fs::create_dir_all(&root)?;
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(root.join(".extract.lock"))?;
    lock.lock_exclusive()?;
    let result = (|| {
        let marker = root.join(".complete");
        if marker.is_file() && root.join("schemas").is_dir() {
            return Ok(root.clone());
        }
        let schemas = root.join("schemas");
        if schemas.exists() {
            fs::remove_dir_all(&schemas)?;
        }
        fs::create_dir_all(&schemas)?;
        SCHEMAS.extract(&schemas)?;
        fs::write(marker, b"ok\n")?;
        Ok(root.clone())
    })();
    let _ = FileExt::unlock(&lock);
    result.map_err(|err: std::io::Error| {
        RainyError::config(
            "BUNDLED_ASSET_EXTRACTION_FAILED",
            format!("cannot prepare bundled runtime assets: {err}"),
        )
    })
}
