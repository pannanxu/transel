//! 系统钥匙串抽象：API Key 不再落盘到 JSON，只在钥匙串里保存。
//!
//! 设计：
//! - 配置文件中 `key_ref` 字段是 `tr://<service>/<account>` 格式的占位符。
//! - 真正取值时调 [`SecretStore::get`]，失败则视为"未配置"。
//! - 写值用 [`SecretStore::set`]，同步删除用 [`SecretStore::delete`]。
//! - 通过 [`ref_for_inline`] 可以把用户输入的明文包成同样格式的引用，
//!   然后立即调用 [`SecretStore::set`] 落盘到钥匙串后返回引用，配置里只存引用。
//!
//! service 固定为 `dev.transel`；所有密钥合并进一条 `_vault` 记录，
//! 避免启动时每个账号各弹一次钥匙串密码。进程内再缓存，只解锁一次。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecretError {
    NotFound,
    Backend(String),
}

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretError::NotFound => f.write_str("未配置"),
            SecretError::Backend(s) => write!(f, "钥匙串: {s}"),
        }
    }
}

impl std::error::Error for SecretError {}

pub const SERVICE: &str = "dev.transel";

pub type SecretResult<T> = Result<T, SecretError>;

/// 解析 `key_ref`：`tr://<service>/<account>`，返回 (service, account)。
pub fn parse_ref(r: &str) -> Option<(String, String)> {
    if let Some(rest) = r.strip_prefix("tr://") {
        let (svc, acc) = rest.split_once('/')?;
        Some((svc.to_string(), acc.to_string()))
    } else {
        None
    }
}

/// 生成 `key_ref` 字符串（service 固定，account 是后端 id）。
pub fn make_ref(account: &str) -> String {
    format!("tr://{SERVICE}/{account}")
}

/// 将明文包装成 `key_ref`（仅占位；调用方必须随后 [`SecretStore::set`] 实际写入钥匙串）。
pub fn ref_for_inline(inline: &str) -> String {
    let _ = inline;
    make_ref("inline")
}

/// 后端 id 标准化（与 [`crate::app::config::BackendKind::as_str`] 对齐）。
pub fn account_for(kind: &str) -> String {
    kind.to_string()
}

/// 抽象的钥匙串存储，方便单测里注入内存实现。
pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> SecretResult<String>;
    fn set(&self, account: &str, value: &str) -> SecretResult<()>;
    fn delete(&self, account: &str) -> SecretResult<()>;
}

/// 默认实现：使用 `keyring` crate。失败时返回 [`SecretError::Backend`]。
pub struct KeyringStore;

const VAULT_ACCOUNT: &str = "_vault";

struct VaultCache {
    loaded: bool,
    map: std::collections::BTreeMap<String, String>,
}

static VAULT: parking_lot::Mutex<VaultCache> = parking_lot::Mutex::new(VaultCache {
    loaded: false,
    map: std::collections::BTreeMap::new(),
});

fn keyring_entry(account: &str) -> SecretResult<keyring::Entry> {
    keyring::Entry::new(SERVICE, account).map_err(|e| SecretError::Backend(e.to_string()))
}

fn keyring_get_raw(account: &str) -> SecretResult<String> {
    keyring_entry(account)?.get_password().map_err(|e| match e {
        keyring::Error::NoEntry => SecretError::NotFound,
        other => SecretError::Backend(other.to_string()),
    })
}

fn keyring_set_raw(account: &str, value: &str) -> SecretResult<()> {
    keyring_entry(account)?
        .set_password(value)
        .map_err(|e| SecretError::Backend(e.to_string()))
}

fn keyring_delete_raw(account: &str) -> SecretResult<()> {
    match keyring_entry(account)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(SecretError::Backend(e.to_string())),
    }
}

fn ensure_vault_loaded(v: &mut VaultCache) {
    if v.loaded {
        return;
    }
    v.loaded = true;
    match keyring_get_raw(VAULT_ACCOUNT) {
        Ok(json) => match serde_json::from_str(&json) {
            Ok(map) => v.map = map,
            Err(e) => log::warn!("钥匙串保险库损坏，将重建: {e}"),
        },
        Err(SecretError::NotFound) => {}
        Err(e) => log::warn!("读取钥匙串保险库失败: {e}"),
    }
}

fn persist_vault(map: &std::collections::BTreeMap<String, String>) -> SecretResult<()> {
    let json = serde_json::to_string(map).map_err(|e| SecretError::Backend(e.to_string()))?;
    keyring_set_raw(VAULT_ACCOUNT, &json)
}

impl SecretStore for KeyringStore {
    fn get(&self, account: &str) -> SecretResult<String> {
        if account == VAULT_ACCOUNT {
            return keyring_get_raw(account);
        }
        let mut v = VAULT.lock();
        ensure_vault_loaded(&mut v);
        if let Some(val) = v.map.get(account) {
            return Ok(val.clone());
        }
        match keyring_get_raw(account) {
            Ok(val) => {
                v.map.insert(account.to_string(), val.clone());
                if let Err(e) = persist_vault(&v.map) {
                    log::warn!("迁移密钥到保险库失败: {e}");
                } else {
                    let _ = keyring_delete_raw(account);
                }
                Ok(val)
            }
            Err(e) => Err(e),
        }
    }

    fn set(&self, account: &str, value: &str) -> SecretResult<()> {
        if account == VAULT_ACCOUNT {
            return keyring_set_raw(account, value);
        }
        let mut v = VAULT.lock();
        ensure_vault_loaded(&mut v);
        v.map.insert(account.to_string(), value.to_string());
        persist_vault(&v.map)?;
        let _ = keyring_delete_raw(account);
        Ok(())
    }

    fn delete(&self, account: &str) -> SecretResult<()> {
        if account == VAULT_ACCOUNT {
            return keyring_delete_raw(account);
        }
        let mut v = VAULT.lock();
        ensure_vault_loaded(&mut v);
        v.map.remove(account);
        persist_vault(&v.map)?;
        keyring_delete_raw(account)
    }
}

/// 全局访问点。可以通过 [`set_store`] 在测试里替换。
use std::sync::OnceLock;
static STORE: OnceLock<Box<dyn SecretStore>> = OnceLock::new();

pub fn set_store(store: Box<dyn SecretStore>) {
    // OnceLock 不能改写，只能第一次 set；为此提供一个简单的 Mutex 包装。
    // 这里用 `try_init` 兼容多线程；初始化过一次就忽略。
    let _ = STORE.set(store);
}

fn store() -> &'static dyn SecretStore {
    STORE.get_or_init(|| Box::new(KeyringStore)).as_ref()
}

pub fn get(account: &str) -> SecretResult<String> {
    store().get(account)
}
pub fn set(account: &str, value: &str) -> SecretResult<()> {
    store().set(account, value)
}
pub fn delete(account: &str) -> SecretResult<()> {
    store().delete(account)
}

/// 环境变量优先，再读钥匙串。
pub fn env_or_keyring(env_keys: &[&str], accounts: &[&str]) -> Option<String> {
    for k in env_keys {
        if let Ok(v) = std::env::var(k)
            && !v.is_empty()
        {
            return Some(v);
        }
    }
    for a in accounts {
        if let Ok(v) = get(a)
            && !v.is_empty()
        {
            return Some(v);
        }
    }
    None
}

/// 从 [`crate::app::config::BackendEntry::params`] 取 key：先看 `key`（明文），
/// 再看 `key_ref` 解析到钥匙串。优先级：`key` > `key_ref`。
pub fn lookup_key(
    params: &std::collections::BTreeMap<String, String>,
    default_account: &str,
) -> Option<String> {
    if let Some(inline) = params.get("key")
        && !inline.is_empty()
    {
        return Some(inline.clone());
    }
    if let Some(r) = params.get("key_ref") {
        if let Some((_svc, acc)) = parse_ref(r) {
            return store().get(&acc).ok();
        }
        // 旧 inline 占位：在这种情况下我们允许回退到默认账户查找
        return store().get(default_account).ok();
    }
    None
}
