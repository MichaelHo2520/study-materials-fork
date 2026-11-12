//! User-bound DPAPI ciphertext on Windows; memory-only credentials elsewhere.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_SESSION: u64 = 1024 * 1024;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Session {
    #[serde(default)]
    pub origin: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub user: serde_json::Value,
}

pub struct SessionStore {
    file: Mutex<Option<PathBuf>>,
    current: Mutex<Session>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self { file: Mutex::new(None), current: Mutex::new(Session::default()) }
    }

    pub fn bind(&self, dir: PathBuf) -> Result<(), String> {
        // Never retain or silently reuse legacy plaintext credentials.
        remove_if_present(&dir.join("session.json"))?;
        #[cfg(windows)]
        {
            let path = dir.join("session.dat");
            if let Ok(meta) = std::fs::metadata(&path) {
                if meta.len() <= MAX_SESSION {
                    if let Ok(bytes) = std::fs::read(&path) {
                        // Corrupt/foreign ciphertext means logged out, never plaintext fallback.
                        if let Ok(plain) = crypt(&bytes, false) {
                            if let Ok(session) = serde_json::from_slice(&plain) {
                                *self.current.lock().unwrap() = session;
                            }
                        }
                    }
                }
            }
            *self.file.lock().unwrap() = Some(path);
        }
        Ok(())
    }

    pub fn token(&self, origin: &str) -> Option<String> {
        let cur = self.current.lock().unwrap();
        (cur.origin == origin && !cur.token.is_empty()).then(|| cur.token.clone())
    }

    pub fn user(&self, origin: &str) -> serde_json::Value {
        let cur = self.current.lock().unwrap();
        if cur.origin == origin && !cur.token.is_empty() { cur.user.clone() }
        else { serde_json::Value::Null }
    }

    pub fn set(&self, origin: &str, token: String, user: serde_json::Value) -> Result<(), String> {
        let mut cur = self.current.lock().unwrap();
        let next = Session { origin: origin.into(), token, user };
        if let Some(path) = self.file.lock().unwrap().as_ref() {
            let plain = serde_json::to_vec(&next).map_err(|_| "无法编码会话")?;
            if plain.len() as u64 > MAX_SESSION / 2 { return Err("会话资料过大".into()); }
            save_protected(path, &plain)?;
        }
        *cur = next; // Commit only after persistence succeeds.
        Ok(())
    }

    pub fn clear(&self) -> Result<(), String> {
        let mut cur = self.current.lock().unwrap();
        *cur = Session::default();
        if let Some(path) = self.file.lock().unwrap().as_ref() { remove_if_present(path)?; }
        Ok(())
    }
}

fn remove_if_present(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("无法移除旧会话，请检查应用数据目录权限".into()),
    }
}

#[cfg(windows)]
fn crypt(bytes: &[u8], protect: bool) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len()).map_err(|_| "会话资料过大")?,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output: CRYPT_INTEGER_BLOB = unsafe { std::mem::zeroed() };
    // DPAPI borrows input and allocates output; copy it before LocalFree.
    // No LOCAL_MACHINE flag: only the current Windows user can decrypt it.
    let ok = unsafe {
        if protect {
            CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(),
                std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        } else {
            CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(),
                std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        }
    };
    if ok == 0 { return Err("Windows 无法保护或读取登录会话".into()); }
    let result = if output.cbData == 0 { Vec::new() }
        else { unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() } };
    unsafe { LocalFree(output.pbData.cast()); }
    Ok(result)
}

#[cfg(windows)]
fn save_protected(path: &Path, plain: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
    let encrypted = crypt(plain, true)?;
    static NEXT_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let temp = path.with_extension(format!("pending.{}.{}", std::process::id(),
        NEXT_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)
            .map_err(|_| "无法保存加密会话")?;
        file.write_all(&encrypted).and_then(|_| file.sync_all()).map_err(|_| "无法保存加密会话")?;
        drop(file);
        let source: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err("无法替换加密会话".to_string());
        }
        Ok(())
    })();
    if result.is_err() { let _ = std::fs::remove_file(temp); }
    result
}

#[cfg(not(windows))]
fn save_protected(_path: &Path, _plain: &[u8]) -> Result<(), String> {
    Err("此平台仅支持当次会话".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hghub-session-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&dir).unwrap(); dir
    }
    #[test]
    fn credentials_are_bound_to_origin() {
        let store = SessionStore::new();
        store.set("https://huangguoai.com", "secret-token".into(), json!({"id":1})).unwrap();
        assert_eq!(store.token("https://huangguoai.com").as_deref(), Some("secret-token"));
        assert_eq!(store.token("https://huangguoai.ai"), None);
        assert!(store.user("https://evil.example").is_null());
        store.clear().unwrap();
        assert_eq!(store.token("https://huangguoai.com"), None);
    }
    #[test]
    fn legacy_plaintext_is_removed_and_requires_login() {
        let dir = temp_dir();
        std::fs::write(dir.join("session.json"), r#"{"origin":"https://huangguoai.com","token":"old"}"#).unwrap();
        let store = SessionStore::new(); store.bind(dir.clone()).unwrap();
        assert!(!dir.join("session.json").exists());
        assert_eq!(store.token("https://huangguoai.com"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn encrypted_roundtrip_overwrite_logout_and_failed_save() {
        let dir = temp_dir();
        std::fs::write(dir.join("session.dat"), b"corrupt ciphertext").unwrap();
        let store = SessionStore::new(); store.bind(dir.clone()).unwrap();
        assert_eq!(store.token("https://huangguoai.com"), None);
        let origin = "https://huangguoai.com";
        store.set(origin, "private-token-one".into(), json!({"id":1})).unwrap();
        let disk = std::fs::read(dir.join("session.dat")).unwrap();
        assert!(!disk.windows(b"private-token-one".len()).any(|w| w == b"private-token-one"));
        assert!(serde_json::from_slice::<serde_json::Value>(&disk).is_err());
        store.set(origin, "private-token-two".into(), json!({"id":2})).unwrap();
        let reopened = SessionStore::new(); reopened.bind(dir.clone()).unwrap();
        assert_eq!(reopened.token(origin).as_deref(), Some("private-token-two"));
        *store.file.lock().unwrap() = Some(dir.join("missing/session.dat"));
        assert!(store.set(origin, "lost".into(), json!({})).is_err());
        assert_eq!(store.token(origin).as_deref(), Some("private-token-two"));
        reopened.clear().unwrap(); assert!(!dir.join("session.dat").exists());
        let fresh = SessionStore::new(); fresh.bind(dir.clone()).unwrap();
        assert_eq!(fresh.token(origin), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(not(windows))]
    #[test]
    fn unsupported_platform_never_persists_credentials() {
        let dir = temp_dir(); let store = SessionStore::new(); store.bind(dir.clone()).unwrap();
        store.set("https://huangguoai.com", "memory-only".into(), json!({})).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let fresh = SessionStore::new(); fresh.bind(dir.clone()).unwrap();
        assert_eq!(fresh.token("https://huangguoai.com"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
