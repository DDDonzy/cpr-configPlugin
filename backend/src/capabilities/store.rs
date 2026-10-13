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
    fn io(error: std::io::Error) -> Self {
        Self {
            status: 503,
            message: match error.kind() {
                std::io::ErrorKind::NotFound => "基础配置文件缺失。若曾保存过，请检查路径或恢复备份；不会覆盖或自动重建历史文件。",
                std::io::ErrorKind::PermissionDenied => "无权读取或写入基础配置文件，请检查插件运行用户及目录/文件权限；不会覆盖现有文件。",
                _ => "配置文件操作未完成，请重新加载检查；不会自动覆盖现有文件。",
            }.into(),
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
    pub initialized: bool,
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
    fn fresh_location(&self) -> Result<bool, StoreError> {
        Self::reject_symlink(&self.root)?;
        Self::reject_symlink(&self.file())?;
        if self.file().try_exists().map_err(StoreError::io)? {
            return Ok(false);
        }
        if !self.root.try_exists().map_err(StoreError::io)? {
            return Ok(true);
        }
        // Any prior lock, backup, marker or unknown data makes this a recovery case.
        Ok(fs::read_dir(&self.root)
            .map_err(StoreError::io)?
            .next()
            .is_none())
    }
    pub fn initialize(&self) -> Result<Snapshot, StoreError> {
        for path in self.root.ancestors() {
            Self::reject_symlink(path)?;
        }
        if !self.fresh_location()? {
            return Err(StoreError {
                status: 409,
                message: "已有配置或历史记录，拒绝首次初始化；请重新加载或恢复备份。".into(),
            });
        }
        let mut dir = fs::DirBuilder::new();
        dir.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            dir.mode(0o700);
        }
        dir.create(&self.root).map_err(StoreError::io)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| StoreError::invalid("系统时间无效"))?
            .as_nanos();
        let temp = self
            .root
            .join(format!(".initialize-{}-{stamp}.tmp", std::process::id()));
        let result = (|| {
            let mut file = private_file(&temp)?;
            file.write_all(b"{}\n").map_err(StoreError::io)?;
            file.sync_all().map_err(StoreError::io)?;
            // Atomic no-overwrite publication. Concurrent initialization can never replace a file.
            fs::hard_link(&temp, self.file()).map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    StoreError::conflict()
                } else {
                    StoreError::io(e)
                }
            })?;
            Ok(())
        })();
        let _ = fs::remove_file(&temp);
        result?;
        self.snapshot()
    }
    pub fn snapshot(&self) -> Result<Snapshot, StoreError> {
        if self.fresh_location()? {
            return Ok(Snapshot {
                version: "0".repeat(64),
                models: Catalog::default(),
                model_order: vec![],
                file_path: self.file().display().to_string(),
                plugin_version: env!("CARGO_PKG_VERSION"),
                initialized: false,
            });
        }
        let bytes = self.raw()?;
        let models = Catalog::parse(&bytes).map_err(StoreError::invalid)?;
        Ok(Snapshot {
            version: digest(&bytes),
            model_order: models.0.keys().cloned().collect(),
            models,
            file_path: self.file().display().to_string(),
            plugin_version: env!("CARGO_PKG_VERSION"),
            initialized: true,
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
    fn first_install_read_is_virtual_and_never_writes() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("new");
        let store = Store::new(root.clone());
        let snap = store.snapshot().unwrap();
        assert!(!snap.initialized);
        assert!(snap.models.0.is_empty());
        assert_eq!(snap.version, "0".repeat(64));
        assert!(!root.exists());
    }
    #[test]
    fn explicit_initialization_creates_empty_once() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("new");
        let store = Store::new(root.clone());
        let snap = store.initialize().unwrap();
        assert!(snap.initialized);
        assert!(snap.models.0.is_empty());
        assert_eq!(fs::read(store.file()).unwrap(), b"{}\n");
        assert_eq!(store.initialize().unwrap_err().status, 409);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&root).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(store.file()).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn history_without_source_is_not_first_install() {
        let (dir, store, mut catalog) = setup();
        let snap = store.snapshot().unwrap();
        catalog.0.get_mut("a").unwrap().display_name = "Saved".into();
        store.save(&snap.version, &catalog).unwrap();
        fs::remove_file(store.file()).unwrap();
        assert_eq!(store.snapshot().unwrap_err().status, 503);
        assert_eq!(store.initialize().unwrap_err().status, 409);
        assert!(dir.path().join("backups").exists());
        assert!(!store.file().exists());
    }
    #[test]
    fn initialization_never_overwrites_valid_or_corrupt_data() {
        let (_dir, store, _) = setup();
        let old = fs::read(store.file()).unwrap();
        assert_eq!(store.initialize().unwrap_err().status, 409);
        assert_eq!(fs::read(store.file()).unwrap(), old);
        fs::write(store.file(), b"bad JSON").unwrap();
        assert_eq!(store.initialize().unwrap_err().status, 409);
        assert_eq!(fs::read(store.file()).unwrap(), b"bad JSON");
    }
    #[test]
    fn concurrent_first_initializations_never_replace_data() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("new");
        let a = Store::new(root.clone());
        let b = Store::new(root);
        a.initialize().unwrap();
        let snap = a.snapshot().unwrap();
        let catalog=Catalog::parse(br#"{"a":{"display_name":"A","reasoning_levels":["high"],"context_window":1000,"input_modalities":["text"]}}"#).unwrap();
        a.save(&snap.version, &catalog).unwrap();
        let old = fs::read(a.file()).unwrap();
        assert_eq!(b.initialize().unwrap_err().status, 409);
        assert_eq!(fs::read(a.file()).unwrap(), old);
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
