use crate::capabilities::domain::{Catalog, MAX_BYTES};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const ROOT: &str = "/app/.runtime/data/model-capabilities";
#[derive(Debug)]
pub struct StoreError {
    pub status: u16,
    pub message: String,
}
impl StoreError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            status: 400,
            message: message.into(),
        }
    }
    fn io(_: std::io::Error) -> Self {
        Self {
            status: 503,
            message:
                "配置文件操作未完成，请重新加载检查。若文件已损坏，请从备份恢复，不会自动覆盖。"
                    .into(),
        }
    }
    fn conflict() -> Self {
        Self {
            status: 409,
            message: "文件已被其他页面或进程修改，请重新加载再编辑".into(),
        }
    }
}
#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub version: String,
    pub models: Catalog,
    pub model_order: Vec<String>,
    pub file_path: String,
    pub plugin_version: &'static str,
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn private_file(path: &Path) -> Result<File, StoreError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(StoreError::io)
}
impl Default for Store {
    fn default() -> Self {
        #[cfg(debug_assertions)]
        if let Some(root) = std::env::var_os("CPR_MODEL_CAPABILITIES_TEST_DIR") {
            return Self::new(PathBuf::from(root));
        }
        Self::new(PathBuf::from(ROOT))
    }
}
impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    fn file(&self) -> PathBuf {
        self.root.join("models.common.json")
    }
    fn reject_symlink(path: &Path) -> Result<(), StoreError> {
        match fs::symlink_metadata(path) {
            Ok(m) if m.file_type().is_symlink() => {
                Err(StoreError::invalid("配置路径不能是符号链接"))
            }
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StoreError::io(e)),
        }
    }
    fn raw(&self) -> Result<Vec<u8>, StoreError> {
        Self::reject_symlink(&self.root)?;
        Self::reject_symlink(&self.file())?;
        let f = File::open(self.file()).map_err(StoreError::io)?;
        if !f.metadata().map_err(StoreError::io)?.is_file() {
            return Err(StoreError::invalid("配置路径不是文件"));
        }
        let mut bytes = Vec::new();
        f.take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(StoreError::io)?;
        if bytes.len() > MAX_BYTES {
            return Err(StoreError::invalid("配置文件过大"));
        }
        Ok(bytes)
    }
    pub fn snapshot(&self) -> Result<Snapshot, StoreError> {
        let bytes = self.raw()?;
        let models = Catalog::parse(&bytes).map_err(StoreError::invalid)?;
        Ok(Snapshot {
            version: digest(&bytes),
            model_order: models.0.keys().cloned().collect(),
            models,
            file_path: self.file().display().to_string(),
            plugin_version: env!("CARGO_PKG_VERSION"),
        })
    }
    pub fn preview(
        &self,
        expected: &str,
        models: &Catalog,
    ) -> Result<(Snapshot, crate::capabilities::domain::Diff), StoreError> {
        models.validate().map_err(StoreError::invalid)?;
        let snapshot = self.snapshot()?;
        if snapshot.version != expected {
            return Err(StoreError::conflict());
        }
        let diff = crate::capabilities::domain::diff(&snapshot.models, models);
        Ok((snapshot, diff))
    }
    pub fn save(&self, expected: &str, models: &Catalog) -> Result<Snapshot, StoreError> {
        models.validate().map_err(StoreError::invalid)?;
        let lock_path = self.root.join(".write.lock");
        Self::reject_symlink(&self.root)?;
        Self::reject_symlink(&lock_path)?;
        let mut lock_opts = OpenOptions::new();
        lock_opts
            .read(true)
            .write(true)
            .create(true)
            .truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            lock_opts.mode(0o600);
        }
        let lock = lock_opts.open(lock_path).map_err(StoreError::io)?;
        lock.try_lock().map_err(|_| StoreError {
            status: 409,
            message: "另一个保存正在进行，请稍后重新加载".into(),
        })?;
        let original = self.raw()?;
        if digest(&original) != expected {
            return Err(StoreError::conflict());
        }
        let previous = Catalog::parse(&original).map_err(StoreError::invalid)?;
        if previous == *models {
            return self.snapshot();
        }
        let rendered = models.render().map_err(StoreError::invalid)?;
        let backups = self.root.join("backups");
        Self::reject_symlink(&backups)?;
        fs::create_dir_all(&backups).map_err(StoreError::io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&backups, fs::Permissions::from_mode(0o700))
                .map_err(StoreError::io)?;
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| StoreError::invalid("系统时间无效"))?
            .as_nanos();
        let backup = backups.join(format!("catalog-{nonce}-{}.json", &expected[..12]));
        let mut f = private_file(&backup)?;
        f.write_all(&original).map_err(StoreError::io)?;
        f.sync_all().map_err(StoreError::io)?;
        let temp = self.root.join(format!(".catalog-{nonce}.tmp"));
        let result = (|| {
            let mut f = private_file(&temp)?;
            f.write_all(rendered.as_bytes()).map_err(StoreError::io)?;
            f.sync_all().map_err(StoreError::io)?;
            drop(f);
            if self.raw()? != original {
                return Err(StoreError::conflict());
            }
            fs::rename(&temp, self.file()).map_err(StoreError::io)?;
            #[cfg(unix)]
            File::open(&self.root)
                .and_then(|f| f.sync_all())
                .map_err(StoreError::io)?;
            self.snapshot()
        })();
        if result.is_err() && temp.exists() {
            let _ = fs::remove_file(temp);
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, Store, Catalog) {
        let d = tempfile::tempdir().unwrap();
        let c=Catalog::parse(br#"{"a":{"display_name":"A","reasoning_levels":["high"],"context_window":1000,"input_modalities":["text"]}}"#).unwrap();
        fs::write(d.path().join("models.common.json"), c.render().unwrap()).unwrap();
        let s = Store::new(d.path().to_owned());
        (d, s, c)
    }
    #[test]
    fn save_and_backup() {
        let (d, s, mut c) = setup();
        let before = s.snapshot().unwrap();
        c.0.get_mut("a").unwrap().display_name = "New".into();
        let after = s.save(&before.version, &c).unwrap();
        assert_ne!(before.version, after.version);
        assert_eq!(after.models, c);
        assert_eq!(fs::read_dir(d.path().join("backups")).unwrap().count(), 1);
    }
    #[test]
    fn stale_save_does_not_write() {
        let (d, s, c) = setup();
        let before = fs::read(s.file()).unwrap();
        assert_eq!(s.save(&"a".repeat(64), &c).unwrap_err().status, 409);
        assert_eq!(fs::read(s.file()).unwrap(), before);
        assert!(!d.path().join("backups").exists());
    }
    #[test]
    fn preview_is_read_only() {
        let (d, s, c) = setup();
        let before = s.snapshot().unwrap();
        s.preview(&before.version, &c).unwrap();
        assert!(!d.path().join("backups").exists());
        assert!(!d.path().join(".write.lock").exists());
    }
    #[test]
    fn noop_does_not_rewrite() {
        let (d, s, c) = setup();
        let before = s.snapshot().unwrap();
        assert_eq!(s.save(&before.version, &c).unwrap().version, before.version);
        assert!(!d.path().join("backups").exists());
    }
    #[test]
    fn corrupted_file_is_not_replaced() {
        let (_d, s, c) = setup();
        fs::write(s.file(), b"broken").unwrap();
        assert!(s.save(&digest(b"broken"), &c).is_err());
        assert_eq!(fs::read(s.file()).unwrap(), b"broken");
    }
    #[test]
    fn two_instances_cannot_overwrite_same_version() {
        let (_d, s, mut c) = setup();
        let version = s.snapshot().unwrap().version;
        c.0.get_mut("a").unwrap().display_name = "First".into();
        s.save(&version, &c).unwrap();
        let other = Store::new(s.root.clone());
        c.0.get_mut("a").unwrap().display_name = "Second".into();
        assert_eq!(other.save(&version, &c).unwrap_err().status, 409);
        assert_eq!(s.snapshot().unwrap().models.0["a"].display_name, "First");
    }
    #[test]
    fn reorder_only_is_written_and_backed_up() {
        let (d, s, mut c) = setup();
        c.0.insert("b".into(), c.0["a"].clone());
        fs::write(s.file(), c.render().unwrap()).unwrap();
        let before = s.snapshot().unwrap();
        let reordered = c.ordered(&["b".into(), "a".into()]).unwrap();
        let after = s.save(&before.version, &reordered).unwrap();
        assert_ne!(before.version, after.version);
        assert_eq!(after.model_order, vec!["b", "a"]);
        assert_eq!(fs::read_dir(d.path().join("backups")).unwrap().count(), 1);
        assert_eq!(
            Catalog::parse(&fs::read(s.file()).unwrap()).unwrap(),
            reordered
        );
    }
}
