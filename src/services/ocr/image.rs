use std::path::Path;

use anyhow::{Context as _, Result};
use base64::Engine;

pub fn read_base64(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("读取图片失败: {}", path.display()))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}
