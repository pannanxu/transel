//! 各翻译后端实现 + 一个简单注册表 [`BackendRegistry`]。

pub mod aliyun;
pub mod apple;
pub mod baidu;
pub mod caiyun;
pub mod deepl;
pub mod dictionary;
pub mod google;
pub mod google_free;
pub mod microsoft;
pub mod openai_compat;
pub mod registry;
pub mod tencent;
pub mod volcano;
pub mod youdao;
