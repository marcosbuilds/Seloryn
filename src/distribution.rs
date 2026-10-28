use super::json::{self, JsonValue};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const STATE_SCHEMA: u64 = 1;
pub const MANIFEST_SCHEMA: u64 = 1;
pub const RUNTIME_FILE: &str = "seloryn.exe";
pub const LAUNCHER_FILE: &str = "seloryn.exe";

#[derive(Debug)]
pub struct DistributionError {
    pub code: &'static str,
    pub message: String,
}

impl DistributionError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DistributionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} {}", self.code, self.message)
    }
}

impl std::error::Error for DistributionError {}

impl From<io::Error> for DistributionError {
    fn from(error: io::Error) -> Self {
        Self::new("SEL-D001", error.to_string())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledVersion {
    pub version: String,
    pub language_version: String,
    pub runtime_path: PathBuf,
    pub sha256: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InstallationState {
    pub default_version: Option<String>,
    pub versions: Vec<String>,
}

pub struct RegistryLock {
    path: PathBuf,
}

impl Drop for RegistryLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn root_dir() -> Result<PathBuf, DistributionError> {
    if let Some(value) = env::var_os("SELORYN_HOME") {
        let path = PathBuf::from(value);
        if path.as_os_str().is_empty() {
            return Err(DistributionError::new("SEL-D002", "SELORYN_HOME is empty"));
        }
        return Ok(path);
    }
    #[cfg(windows)]
    {
        if let Some(value) = env::var_os("LOCALAPPDATA") {
            return Ok(PathBuf::from(value).join("Seloryn"));
        }
        Err(DistributionError::new(
            "SEL-D002",
            "LOCALAPPDATA is unavailable; set SELORYN_HOME for a portable installation",
        ))
    }
    #[cfg(not(windows))]
    {
        if let Some(value) = env::var_os("XDG_DATA_HOME") {
            return Ok(PathBuf::from(value).join("Seloryn"));
        }
        if let Some(value) = env::var_os("HOME") {
            return Ok(PathBuf::from(value).join(".local/share/Seloryn"));
        }
        Err(DistributionError::new(
            "SEL-D002",
            "no per-user data directory is available",
        ))
    }
}

pub fn canonical_version(value: &str) -> Result<String, DistributionError> {
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || (part.len() > 1 && part.starts_with('0'))
                || !part.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(DistributionError::new(
            "SEL-D003",
            format!("version `{value}` must use canonical M.m.p form"),
        ));
    }
    Ok(value.to_owned())
}

pub fn language_version(value: &str) -> Result<String, DistributionError> {
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || (part.len() > 1 && part.starts_with('0'))
                || !part.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(DistributionError::new(
            "SEL-D003",
            format!("language version `{value}` must use canonical M.mm form"),
        ));
    }
    Ok(value.to_owned())
}

pub fn state_path(root: &Path) -> PathBuf {
    root.join("state").join("installations.json")
}

pub fn version_dir(root: &Path, version: &str) -> Result<PathBuf, DistributionError> {
    let version = canonical_version(version)?;
    Ok(root.join("versions").join(version))
}

pub fn versioned_runtime_path(root: &Path, version: &str) -> Result<PathBuf, DistributionError> {
    Ok(version_dir(root, version)?.join("bin").join(RUNTIME_FILE))
}

pub fn launcher_path(root: &Path) -> PathBuf {
    root.join("bin").join(LAUNCHER_FILE)
}

pub fn versioned_launcher_path(root: &Path, version: &str) -> Result<PathBuf, DistributionError> {
    let version = canonical_version(version)?;
    Ok(root.join("bin").join(format!("seloryn{version}.exe")))
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

pub fn sha256_file(path: &Path) -> Result<String, DistributionError> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub fn acquire_lock(root: &Path) -> Result<RegistryLock, DistributionError> {
    let lock_path = root.join("state").join("installations.lock");
    fs::create_dir_all(lock_path.parent().expect("lock has parent"))?;
    for _ in 0..100 {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                writeln!(file, "pid={}", std::process::id())?;
                file.sync_all()?;
                return Ok(RegistryLock { path: lock_path });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if lock_is_stale(&lock_path) {
                    let _ = fs::remove_file(&lock_path);
                } else {
                    thread::sleep(Duration::from_millis(25));
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err(DistributionError::new(
        "SEL-D004",
        "installation registry is locked by another process",
    ))
}

fn lock_is_stale(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default()
        > Duration::from_secs(600)
}

pub fn read_state(root: &Path) -> Result<InstallationState, DistributionError> {
    let path = state_path(root);
    if !path.is_file() {
        return Ok(InstallationState::default());
    }
    let text = fs::read_to_string(&path)?;
    parse_state(&text)
}

pub fn write_state(root: &Path, state: &InstallationState) -> Result<(), DistributionError> {
    let mut versions = state.versions.clone();
    versions.sort();
    versions.dedup();
    let mut object = BTreeMap::new();
    object.insert(
        "schema".to_owned(),
        JsonValue::Number(STATE_SCHEMA.to_string()),
    );
    object.insert(
        "default_version".to_owned(),
        state
            .default_version
            .clone()
            .map(JsonValue::String)
            .unwrap_or(JsonValue::Null),
    );
    object.insert(
        "versions".to_owned(),
        JsonValue::Array(versions.into_iter().map(JsonValue::String).collect()),
    );
    atomic_write(
        &state_path(root),
        json::encode(&JsonValue::Object(object)).as_bytes(),
    )
}

fn parse_state(text: &str) -> Result<InstallationState, DistributionError> {
    let value = json::parse(text, 1024 * 1024, 8)
        .map_err(|error| DistributionError::new("SEL-D005", error.message))?;
    let object = as_object(&value)?;
    let schema = number(object, "schema")?;
    if schema != STATE_SCHEMA {
        return Err(DistributionError::new(
            "SEL-D005",
            "unsupported installation state schema",
        ));
    }
    let default_version = match object.get("default_version") {
        Some(JsonValue::Null) | None => None,
        Some(JsonValue::String(value)) => Some(canonical_version(value)?),
        _ => {
            return Err(DistributionError::new(
                "SEL-D005",
                "default_version must be a version or null",
            ));
        }
    };
    let values = match object.get("versions") {
        Some(JsonValue::Array(values)) => values,
        _ => {
            return Err(DistributionError::new(
                "SEL-D005",
                "versions must be an array",
            ));
        }
    };
    let mut versions = Vec::with_capacity(values.len());
    for value in values {
        let version = match value {
            JsonValue::String(value) => canonical_version(value)?,
            _ => {
                return Err(DistributionError::new(
                    "SEL-D005",
                    "versions must contain strings",
                ));
            }
        };
        if !versions.contains(&version) {
            versions.push(version);
        }
    }
    if let Some(default) = &default_version
        && !versions.contains(default)
    {
        return Err(DistributionError::new(
            "SEL-D005",
            "default_version is not installed",
        ));
    }
    Ok(InstallationState {
        default_version,
        versions,
    })
}

pub fn read_installed(root: &Path, version: &str) -> Result<InstalledVersion, DistributionError> {
    let version = canonical_version(version)?;
    let directory = version_dir(root, &version)?;
    let manifest_path = directory.join("manifest.json");
    let document = fs::read_to_string(&manifest_path).map_err(|_| {
        DistributionError::new(
            "SEL-D006",
            format!("installation manifest is missing for {version}"),
        )
    })?;
    let parsed = json::parse(&document, 1024 * 1024, 8)
        .map_err(|error| DistributionError::new("SEL-D006", error.message))?;
    let object = as_object(&parsed)?;
    if number(object, "schema")? != MANIFEST_SCHEMA {
        return Err(DistributionError::new(
            "SEL-D006",
            "unsupported runtime manifest schema",
        ));
    }
    let manifest_version = string(object, "runtime_package")?;
    if manifest_version != version {
        return Err(DistributionError::new(
            "SEL-D006",
            "manifest version does not match its directory",
        ));
    }
    let language = language_version(string(object, "language_version")?)?;
    let relative = string(object, "binary_relative")?;
    if relative != "bin/seloryn.exe" {
        return Err(DistributionError::new(
            "SEL-D006",
            "runtime path is not canonical",
        ));
    }
    let expected_hash = string(object, "sha256")?.to_ascii_lowercase();
    if expected_hash.len() != 64 || !expected_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(DistributionError::new(
            "SEL-D006",
            "runtime manifest has an invalid SHA-256",
        ));
    }
    let runtime_path = contained_path(&directory, Path::new(relative))?;
    if !runtime_path.is_file() {
        return Err(DistributionError::new(
            "SEL-D006",
            "installed runtime is missing",
        ));
    }
    let actual_hash = sha256_file(&runtime_path)?;
    if actual_hash != expected_hash {
        return Err(DistributionError::new(
            "SEL-D006",
            "installed runtime hash does not match its manifest",
        ));
    }
    Ok(InstalledVersion {
        version,
        language_version: language,
        runtime_path,
        sha256: actual_hash,
    })
}

pub fn install_runtime(
    root: &Path,
    version: &str,
    language: &str,
    runtime_bytes: &[u8],
    launcher_bytes: &[u8],
    set_default: bool,
) -> Result<InstalledVersion, DistributionError> {
    let version = canonical_version(version)?;
    let language = language_version(language)?;
    if runtime_bytes.is_empty() || launcher_bytes.is_empty() {
        return Err(DistributionError::new(
            "SEL-D007",
            "installer payload is empty",
        ));
    }
    fs::create_dir_all(root)?;
    let _lock = acquire_lock(root)?;
    let final_dir = version_dir(root, &version)?;
    if final_dir.exists() {
        let existing = read_installed(root, &version)?;
        if existing.sha256 != sha256_bytes(runtime_bytes) {
            return Err(DistributionError::new(
                "SEL-D008",
                "version is already installed with different bytes",
            ));
        }
        ensure_launchers(root, &version, launcher_bytes, set_default)?;
        let mut state = read_state(root)?;
        if !state.versions.contains(&version) {
            state.versions.push(version.clone());
        }
        if set_default {
            state.default_version = Some(version.clone());
        }
        write_state(root, &state)?;
        return Ok(existing);
    }
    let versions_dir = root.join("versions");
    fs::create_dir_all(&versions_dir)?;
    let staging = versions_dir.join(format!(
        ".{version}.staging-{}-{}",
        std::process::id(),
        timestamp_nanos()
    ));
    let staging_bin = staging.join("bin");
    fs::create_dir_all(&staging_bin)?;
    let runtime_path = staging_bin.join(RUNTIME_FILE);
    atomic_write(&runtime_path, runtime_bytes)?;
    let hash = sha256_file(&runtime_path)?;
    let mut manifest = BTreeMap::new();
    manifest.insert(
        "schema".to_owned(),
        JsonValue::Number(MANIFEST_SCHEMA.to_string()),
    );
    manifest.insert(
        "artifact".to_owned(),
        JsonValue::String("seloryn-runtime".to_owned()),
    );
    manifest.insert(
        "runtime_package".to_owned(),
        JsonValue::String(version.clone()),
    );
    manifest.insert("language_version".to_owned(), JsonValue::String(language));
    manifest.insert(
        "binary_relative".to_owned(),
        JsonValue::String("bin/seloryn.exe".to_owned()),
    );
    manifest.insert("sha256".to_owned(), JsonValue::String(hash.clone()));
    manifest.insert("installed_utc".to_owned(), JsonValue::String(now_utc()));
    atomic_write(
        &staging.join("manifest.json"),
        json::encode(&JsonValue::Object(manifest)).as_bytes(),
    )?;
    if let Err(error) = fs::rename(&staging, &final_dir) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error.into());
    }
    ensure_launchers(root, &version, launcher_bytes, set_default)?;
    let mut state = read_state(root)?;
    if !state.versions.contains(&version) {
        state.versions.push(version.clone());
    }
    if set_default {
        state.default_version = Some(version.clone());
    }
    if let Err(error) = write_state(root, &state) {
        let _ = fs::remove_dir_all(&final_dir);
        return Err(error);
    }
    read_installed(root, &version)
}

fn ensure_launchers(
    root: &Path,
    version: &str,
    launcher_bytes: &[u8],
    replace_default: bool,
) -> Result<(), DistributionError> {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir)?;
    let expected_hash = sha256_bytes(launcher_bytes);
    let versioned = versioned_launcher_path(root, version)?;
    if versioned.exists() {
        if sha256_file(&versioned)? != expected_hash {
            return Err(DistributionError::new(
                "SEL-D009",
                format!("existing launcher bytes differ at {}", versioned.display()),
            ));
        }
    } else {
        atomic_write(&versioned, launcher_bytes)?;
    }

    let default = launcher_path(root);
    if replace_default {
        if !default.exists() || sha256_file(&default)? != expected_hash {
            atomic_write(&default, launcher_bytes)?;
        }
    } else if default.exists() {
        if sha256_file(&default)? != expected_hash {
            return Err(DistributionError::new(
                "SEL-D009",
                format!("existing launcher bytes differ at {}", default.display()),
            ));
        }
    } else {
        atomic_write(&default, launcher_bytes)?;
    }
    Ok(())
}

pub fn set_default(root: &Path, version: &str) -> Result<(), DistributionError> {
    let version = canonical_version(version)?;
    let _ = read_installed(root, &version)?;
    let _lock = acquire_lock(root)?;
    let mut state = read_state(root)?;
    if !state.versions.contains(&version) {
        return Err(DistributionError::new(
            "SEL-D010",
            "version is not registered",
        ));
    }
    state.default_version = Some(version);
    write_state(root, &state)
}

pub fn uninstall(root: &Path, version: &str) -> Result<(), DistributionError> {
    let version = canonical_version(version)?;
    let _lock = acquire_lock(root)?;
    let mut state = read_state(root)?;
    if state.default_version.as_deref() == Some(&version) {
        return Err(DistributionError::new(
            "SEL-D011",
            "select another default before uninstalling the active version",
        ));
    }
    if !state.versions.contains(&version) {
        return Err(DistributionError::new(
            "SEL-D012",
            "version is not installed",
        ));
    }
    let directory = version_dir(root, &version)?;
    let _ = read_installed(root, &version)?;
    fs::remove_dir_all(&directory)?;
    let alias = versioned_launcher_path(root, &version)?;
    let _ = fs::remove_file(alias);
    state.versions.retain(|item| item != &version);
    write_state(root, &state)
}

pub fn resolve_version(
    root: &Path,
    requested: Option<&str>,
) -> Result<InstalledVersion, DistributionError> {
    let state = read_state(root)?;
    let version = match requested {
        Some(value) => canonical_version(value)?,
        None => state.default_version.ok_or_else(|| {
            DistributionError::new("SEL-D013", "no default Seloryn version is selected")
        })?,
    };
    if !state.versions.contains(&version) {
        return Err(DistributionError::new(
            "SEL-D013",
            format!("version `{version}` is not installed"),
        ));
    }
    read_installed(root, &version)
}

pub fn list(root: &Path) -> Result<Vec<InstalledVersion>, DistributionError> {
    let state = read_state(root)?;
    state
        .versions
        .iter()
        .map(|version| read_installed(root, version))
        .collect()
}

pub fn invoked_version(program: &OsStr) -> Result<Option<String>, DistributionError> {
    let name = Path::new(program)
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or_default();
    let Some(suffix) = name.strip_prefix("seloryn") else {
        return Ok(None);
    };
    if suffix.is_empty() {
        return Ok(None);
    }
    Ok(Some(canonical_version(suffix)?))
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), DistributionError> {
    let parent = path
        .parent()
        .ok_or_else(|| DistributionError::new("SEL-D014", "target path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.tmp-{}-{}",
        path.file_name().and_then(OsStr::to_str).unwrap_or("state"),
        std::process::id(),
        timestamp_nanos()
    ));
    let result = (|| -> Result<(), DistributionError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        replace_file(&temporary, path)?;
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn replace_file(source: &Path, destination: &Path) -> Result<(), DistributionError> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        let mut source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut destination_wide: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let ok = unsafe {
            MoveFileExW(
                source_wide.as_mut_ptr(),
                destination_wide.as_mut_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, destination)?;
        Ok(())
    }
}

fn contained_path(root: &Path, relative: &Path) -> Result<PathBuf, DistributionError> {
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(DistributionError::new(
            "SEL-D015",
            "manifest path escapes its version directory",
        ));
    }
    Ok(root.join(relative))
}

fn as_object(value: &JsonValue) -> Result<&BTreeMap<String, JsonValue>, DistributionError> {
    match value {
        JsonValue::Object(object) => Ok(object),
        _ => Err(DistributionError::new(
            "SEL-D005",
            "JSON document must be an object",
        )),
    }
}

fn string<'a>(
    object: &'a BTreeMap<String, JsonValue>,
    key: &str,
) -> Result<&'a str, DistributionError> {
    match object.get(key) {
        Some(JsonValue::String(value)) => Ok(value),
        _ => Err(DistributionError::new(
            "SEL-D005",
            format!("{key} must be a string"),
        )),
    }
}

fn number(object: &BTreeMap<String, JsonValue>, key: &str) -> Result<u64, DistributionError> {
    match object.get(key) {
        Some(JsonValue::Number(value)) => value
            .parse()
            .map_err(|_| DistributionError::new("SEL-D005", format!("{key} must be an integer"))),
        _ => Err(DistributionError::new(
            "SEL-D005",
            format!("{key} must be an integer"),
        )),
    }
}

fn timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn now_utc() -> String {
    format!("{}", timestamp_nanos())
}

#[cfg(test)]
mod tests {
    use super::json::JsonValue;
    use super::{
        InstallationState, canonical_version, contained_path, install_runtime, language_version,
        parse_state, read_state, timestamp_nanos, versioned_launcher_path, write_state,
    };
    use std::collections::BTreeMap;
    use std::path::Path;

    #[test]
    fn accepts_only_full_package_versions() {
        assert_eq!(canonical_version("0.12.0").expect("canonical"), "0.12.0");
        assert!(canonical_version("0.12").is_err());
        assert!(canonical_version("00.12.0").is_err());
        assert!(canonical_version("0.12.0-beta").is_err());
        assert_eq!(language_version("0.12").expect("canonical"), "0.12");
    }

    #[test]
    fn rejects_manifest_path_escape() {
        assert!(contained_path(Path::new("root"), Path::new("bin/seloryn.exe")).is_ok());
        assert!(contained_path(Path::new("root"), Path::new("../outside.exe")).is_err());
        assert!(contained_path(Path::new("root"), Path::new("C:\\outside.exe")).is_err());
    }

    #[test]
    fn parses_and_validates_state_instead_of_guessing() {
        let text = r#"{"schema":1,"default_version":"0.12.0","versions":["0.12.0"]}"#;
        assert_eq!(
            parse_state(text)
                .expect("valid state")
                .default_version
                .as_deref(),
            Some("0.12.0")
        );
        assert!(parse_state(r#"{"schema":1,"default_version":"0.12.0","versions":[]}"#).is_err());
        let mut object = BTreeMap::new();
        object.insert("schema".to_owned(), JsonValue::Number("2".to_owned()));
        object.insert("versions".to_owned(), JsonValue::Array(Vec::new()));
        let _ = InstallationState::default();
    }

    #[test]
    fn installs_an_isolated_runtime_and_launcher_payload() {
        let root = std::env::temp_dir().join(format!(
            "seloryn-distribution-install-{}",
            timestamp_nanos()
        ));
        let installed = install_runtime(&root, "0.12.0", "0.12", b"runtime", b"launcher", false)
            .expect("install payload");
        assert_eq!(installed.version, "0.12.0");
        assert!(installed.runtime_path.is_file());
        assert!(
            versioned_launcher_path(&root, "0.12.0")
                .expect("launcher path")
                .is_file()
        );
        assert_eq!(read_state(&root).expect("state").default_version, None);
        std::fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn rejects_replacement_and_uninstalls_only_after_default_is_cleared() {
        let root = std::env::temp_dir().join(format!(
            "seloryn-distribution-lifecycle-{}",
            timestamp_nanos()
        ));
        let installed = install_runtime(&root, "0.12.0", "0.12", b"runtime", b"launcher", false)
            .expect("initial install");
        let same = install_runtime(&root, "0.12.0", "0.12", b"runtime", b"launcher", true)
            .expect("idempotent install");
        assert_eq!(same.sha256, installed.sha256);
        assert_eq!(
            read_state(&root).expect("state").default_version,
            Some("0.12.0".to_owned())
        );
        let error = install_runtime(
            &root,
            "0.12.0",
            "0.12",
            b"different-runtime",
            b"launcher",
            false,
        )
        .expect_err("replacement must be rejected");
        assert_eq!(error.code, "SEL-D008");
        assert!(super::uninstall(&root, "0.12.0").is_err());
        write_state(
            &root,
            &InstallationState {
                default_version: None,
                versions: vec!["0.12.0".to_owned()],
            },
        )
        .expect("clear default");
        super::uninstall(&root, "0.12.0").expect("selective uninstall");
        assert!(read_state(&root).expect("state").versions.is_empty());
        std::fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn selecting_a_default_repairs_a_stale_default_launcher() {
        let root =
            std::env::temp_dir().join(format!("seloryn-distribution-repair-{}", timestamp_nanos()));
        install_runtime(&root, "0.12.0", "0.12", b"runtime", b"launcher", false)
            .expect("initial install");
        std::fs::write(super::launcher_path(&root), b"legacy-launcher")
            .expect("write stale default launcher");

        install_runtime(&root, "0.12.0", "0.12", b"runtime", b"launcher", true)
            .expect("repair default launcher");

        assert_eq!(
            std::fs::read(super::launcher_path(&root)).expect("default launcher"),
            b"launcher"
        );
        assert_eq!(
            read_state(&root).expect("state").default_version,
            Some("0.12.0".to_owned())
        );
        std::fs::remove_dir_all(root).expect("remove test root");
    }
}
