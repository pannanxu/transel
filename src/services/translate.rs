//! 翻译服务：可插拔的多个后端 + 并行调用 + 并排对比。
//!
//! - 任何后端实现 [`Translator`] trait；通过 [`register`] 注册。
//! - [`TranslateService::request`] **先枚举通道**再**并行**翻译，通过 [`BatchEvent`]
//!   流式回写（Started → Progress* → Done），UI 可逐通道刷新。
//! - 单后端故障不会阻塞其它后端；超时由 [`ureq`] 的全局 timeout 兜底。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use futures::StreamExt;
use futures::channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::app::config::{BackendEntry, BackendKind, Config};

pub mod backends;

pub use backends::registry::{BackendRegistry, default_registry};

/// 单个翻译结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleResult {
    pub backend: String,
    pub text: String,
    pub source_lang: Option<String>,
    pub elapsed_ms: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 通道已列出、译文尚未返回。
    #[serde(default)]
    pub pending: bool,
}

impl SingleResult {
    pub fn pending_slot(backend: impl Into<String>) -> Self {
        Self {
            backend: backend.into(),
            text: String::new(),
            source_lang: None,
            elapsed_ms: 0,
            ok: false,
            error: None,
            pending: true,
        }
    }
}

/// 一次请求的聚合结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    pub source: String,
    pub prepared: String,
    pub target_lang: String,
    /// 由"并排对比"模式决定：单后端取首个成功 / 多后端全保留。
    pub results: Vec<SingleResult>,
}

impl BatchResult {
    /// 取第一个成功的（用于普通模式显示）。
    pub fn first_ok(&self) -> Option<&SingleResult> {
        self.results.iter().find(|r| r.ok && !r.pending)
    }

    pub fn all_settled(&self) -> bool {
        !self.results.is_empty() && self.results.iter().all(|r| !r.pending)
    }

    pub fn apply_progress(&mut self, result: SingleResult) {
        if let Some(slot) = self
            .results
            .iter_mut()
            .find(|r| r.backend == result.backend)
        {
            *slot = result;
        } else {
            self.results.push(result);
        }
    }
}

/// 流式批次事件：先 Started（通道列表），再逐个 Progress，最后 Done。
#[derive(Debug, Clone)]
pub enum BatchEvent {
    /// 通道已遍历完成，各槽位 `pending=true`
    Started(BatchResult),
    /// 某个通道完成（成功或失败）
    Progress(SingleResult),
    /// 全部通道已结束
    Done,
    /// 无法开始（例如没有启用后端）
    Failed(String),
}

/// 翻译请求上下文。
#[derive(Debug, Clone)]
pub struct RequestCtx {
    pub source: String,
    pub prepared: String,
    pub source_lang_hint: Option<String>,
    pub target_lang: String,
    /// 如果非 None，则不管配置如何，强制只跑这些后端（按名字）。
    pub only: Option<Vec<String>>,
}

pub type Reply = UnboundedSender<BatchEvent>;

/// 流式增量回调：参数为当前累计译文。
/// 返回 `false` 表示调用方已取消，后端应尽快中止。
pub type StreamCallback<'a> = &'a dyn Fn(&str) -> bool;

pub trait Translator: Send + Sync {
    fn name(&self) -> &str;
    fn translate_blocking(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
    ) -> Result<SingleResult>;

    /// 是否支持 token 级流式输出（默认否）。
    fn supports_streaming(&self) -> bool {
        false
    }

    /// 流式翻译；默认退化为一次 [`translate_blocking`]。
    /// `on_delta` 收到的是累计全文；返回 false 时应中止。
    fn translate_streaming(
        &self,
        text: &str,
        target: &str,
        source_hint: Option<&str>,
        on_delta: StreamCallback<'_>,
    ) -> Result<SingleResult> {
        let _ = on_delta;
        self.translate_blocking(text, target, source_hint)
    }
}

struct ServiceMeta {
    target: String,
    source: String,
    auto_swap: bool,
}

type BackendList = Vec<(String, Arc<dyn Translator>)>;
type HistoryCallback = Arc<dyn Fn(crate::app::history::HistoryEntry) + Send + Sync>;

/// 主服务。
pub struct TranslateService {
    backends: Arc<parking_lot::RwLock<BackendList>>,
    meta: parking_lot::RwLock<ServiceMeta>,
    /// 工作线程
    tx: std::sync::mpsc::Sender<(RequestCtx, Reply, Arc<AtomicBool>)>,
    /// 当前批次的取消标志；关闭弹窗时置 true。
    current_cancel: Mutex<Option<Arc<AtomicBool>>>,
    history_cb: parking_lot::Mutex<Option<HistoryCallback>>,
}

impl TranslateService {
    pub fn start(cfg: &Config) -> Arc<Self> {
        Self::start_with_registry(cfg, Arc::new(default_registry()))
    }

    pub fn start_with_registry(cfg: &Config, registry: Arc<BackendRegistry>) -> Arc<Self> {
        let backends = build_backend_list(cfg, &registry);
        log::info!(
            "翻译后端: {:?}",
            backends.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()
        );

        let backends = Arc::new(parking_lot::RwLock::new(backends));
        let (tx, rx) = std::sync::mpsc::channel::<(RequestCtx, Reply, Arc<AtomicBool>)>();
        let worker_backends = backends.clone();
        std::thread::Builder::new()
            .name("translate-worker".into())
            .spawn(move || {
                for (ctx, reply, cancel) in rx {
                    let snap = worker_backends.read().clone();
                    run_streaming(&snap, &ctx, reply, cancel);
                }
            })
            .expect("spawn translate worker");
        Arc::new(Self {
            backends,
            meta: parking_lot::RwLock::new(ServiceMeta {
                target: cfg.target_lang.clone(),
                source: cfg.source_lang.clone(),
                auto_swap: cfg.auto_swap,
            }),
            tx,
            current_cancel: Mutex::new(None),
            history_cb: parking_lot::Mutex::new(None),
        })
    }

    /// 取消当前进行中的翻译批次（关闭弹窗时调用）。
    pub fn cancel_active(&self) {
        if let Some(flag) = self.current_cancel.lock().as_ref() {
            flag.store(true, Ordering::SeqCst);
        }
    }

    /// 重新构建后端列表（设置面板修改后调用）。
    pub fn rebuild(&self, cfg: &Config) {
        self.rebuild_with_registry(cfg, Arc::new(default_registry()));
    }

    pub fn rebuild_with_registry(&self, cfg: &Config, registry: Arc<BackendRegistry>) {
        let backends = build_backend_list(cfg, &registry);
        log::info!(
            "翻译后端已热更新: {:?}",
            backends.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()
        );
        *self.backends.write() = backends;
        let mut meta = self.meta.write();
        meta.target = cfg.target_lang.clone();
        meta.source = cfg.source_lang.clone();
        meta.auto_swap = cfg.auto_swap;
    }

    pub fn target_lang(&self) -> String {
        self.meta.read().target.clone()
    }
    pub fn source_lang(&self) -> String {
        self.meta.read().source.clone()
    }
    pub fn backend_names(&self) -> Vec<String> {
        self.backends
            .read()
            .iter()
            .map(|(n, _)| n.clone())
            .collect()
    }

    pub fn set_history_callback(
        &self,
        cb: Arc<dyn Fn(crate::app::history::HistoryEntry) + Send + Sync>,
    ) {
        *self.history_cb.lock() = Some(cb);
    }

    /// 发起流式请求：先枚举通道，再并行翻译。
    pub fn request(&self, ctx: RequestCtx) -> UnboundedReceiver<BatchEvent> {
        // 新请求前取消上一批，避免关闭/重开后旧结果回灌
        self.cancel_active();
        let cancel = Arc::new(AtomicBool::new(false));
        *self.current_cancel.lock() = Some(cancel.clone());
        let (tx, rx) = mpsc::unbounded();
        if let Err(std::sync::mpsc::SendError((_, tx, _))) = self.tx.send((ctx, tx, cancel)) {
            let _ = tx.unbounded_send(BatchEvent::Failed("翻译线程已退出".into()));
        }
        rx
    }

    /// 等完全部通道后返回最终 [`BatchResult`]（设置探测 / 静默翻译等）。
    pub async fn request_all(ctx: RequestCtx, svc: &Self) -> Result<BatchResult, String> {
        collect_batch(svc.request(ctx)).await
    }
}

/// 消费流式事件，得到最终批次。
pub async fn collect_batch(mut rx: UnboundedReceiver<BatchEvent>) -> Result<BatchResult, String> {
    let mut batch: Option<BatchResult> = None;
    while let Some(ev) = rx.next().await {
        match ev {
            BatchEvent::Started(b) => batch = Some(b),
            BatchEvent::Progress(r) => {
                if let Some(b) = batch.as_mut() {
                    b.apply_progress(r);
                }
            }
            BatchEvent::Done => break,
            BatchEvent::Failed(e) => return Err(e),
        }
    }
    batch.ok_or_else(|| "翻译未返回结果".into())
}

/// 先列出通道并 `Started`，再并行跑各后端，逐个 `Progress`，最后 `Done`。
fn run_streaming(
    backends: &[(String, Arc<dyn Translator>)],
    ctx: &RequestCtx,
    reply: UnboundedSender<BatchEvent>,
    cancel: Arc<AtomicBool>,
) {
    let selected: Vec<(String, Arc<dyn Translator>)> = backends
        .iter()
        .filter(|(name, _)| {
            ctx.only
                .as_ref()
                .map(|only| only.iter().any(|n| n == name))
                .unwrap_or(true)
        })
        .cloned()
        .collect();

    if selected.is_empty() {
        let _ = reply.unbounded_send(BatchEvent::Failed("没有启用的翻译后端".into()));
        return;
    }

    if cancel.load(Ordering::SeqCst) {
        return;
    }

    let skeleton = BatchResult {
        source: ctx.source.clone(),
        prepared: ctx.prepared.clone(),
        target_lang: ctx.target_lang.clone(),
        results: selected
            .iter()
            .map(|(name, _)| SingleResult::pending_slot(name.clone()))
            .collect(),
    };
    if reply.unbounded_send(BatchEvent::Started(skeleton)).is_err() {
        cancel.store(true, Ordering::SeqCst);
        return;
    }

    use std::sync::mpsc as smpsc;
    let (tx, rx) = smpsc::channel::<SingleResult>();
    let mut joins = Vec::new();
    for (name, backend) in &selected {
        let name = name.clone();
        let backend = backend.clone();
        let text = ctx.prepared.clone();
        let target = ctx.target_lang.clone();
        let hint = ctx.source_lang_hint.clone();
        let tx = tx.clone();
        let cancel = cancel.clone();
        joins.push(
            std::thread::Builder::new()
                .name(format!("tr-{name}"))
                .spawn(move || {
                    if cancel.load(Ordering::SeqCst) {
                        return;
                    }
                    let start = Instant::now();
                    let name_for_delta = name.clone();
                    let tx_delta = tx.clone();
                    let cancel_delta = cancel.clone();
                    // 兜住后端内部 panic（如 ureq query 过长 unwrap），避免整次请求丢结果。
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        backend.translate_streaming(&text, &target, hint.as_deref(), &|partial| {
                            if cancel_delta.load(Ordering::SeqCst) {
                                return false;
                            }
                            let _ = tx_delta.send(SingleResult {
                                backend: name_for_delta.clone(),
                                text: partial.to_string(),
                                source_lang: None,
                                elapsed_ms: start.elapsed().as_millis() as u64,
                                ok: true,
                                error: None,
                                pending: true,
                            });
                            !cancel_delta.load(Ordering::SeqCst)
                        })
                    }));
                    if cancel.load(Ordering::SeqCst) {
                        return;
                    }
                    let elapsed = start.elapsed().as_millis() as u64;
                    let mut single = match r {
                        Ok(Ok(mut s)) => {
                            s.backend = name;
                            s.elapsed_ms = elapsed;
                            s.pending = false;
                            s
                        }
                        Ok(Err(e)) => {
                            let msg = format!("{e:#}");
                            if msg.contains("已取消") {
                                return;
                            }
                            SingleResult {
                                backend: name,
                                text: String::new(),
                                source_lang: None,
                                elapsed_ms: elapsed,
                                ok: false,
                                error: Some(msg),
                                pending: false,
                            }
                        }
                        Err(panic) => {
                            let msg = panic
                                .downcast_ref::<&str>()
                                .map(|s| (*s).to_string())
                                .or_else(|| panic.downcast_ref::<String>().cloned())
                                .unwrap_or_else(|| "后端线程 panic".into());
                            SingleResult {
                                backend: name,
                                text: String::new(),
                                source_lang: None,
                                elapsed_ms: elapsed,
                                ok: false,
                                error: Some(msg),
                                pending: false,
                            }
                        }
                    };
                    single.pending = false;
                    let _ = tx.send(single);
                }),
        );
    }
    drop(tx);

    // 按到达顺序立刻推 Progressive（含流式增量）；UI 再按配置顺序展示。
    for r in rx {
        if cancel.load(Ordering::SeqCst) || reply.unbounded_send(BatchEvent::Progress(r)).is_err() {
            cancel.store(true, Ordering::SeqCst);
            break;
        }
    }
    for j in joins.into_iter().flatten() {
        let _ = j.join();
    }
    if !cancel.load(Ordering::SeqCst) {
        let _ = reply.unbounded_send(BatchEvent::Done);
    }
}

// 给后端用的工具
pub(crate) fn ureq_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into()
}

/// 把 `BTreeMap` 风格参数读出。
pub fn param<'a>(p: &'a std::collections::BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    p.get(key).map(|s| s.as_str())
}

/// 给 BackendEntry 派发一个默认显示名。
pub trait BackendEntryExt {
    fn display_name(&self, kind: BackendKind) -> String;
}
impl BackendEntryExt for BackendEntry {
    fn display_name(&self, kind: BackendKind) -> String {
        if !self.display_name.is_empty() {
            return self.display_name.clone();
        }
        kind.default_display_name().into()
    }
}

/// 构建启用的后端列表；同名时追加短 id，避免流式进度写到错误槽位。
fn build_backend_list(
    cfg: &Config,
    registry: &BackendRegistry,
) -> Vec<(String, Arc<dyn Translator>)> {
    use std::collections::HashSet;
    let mut used = HashSet::new();
    cfg.backends
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| {
            let t = registry.build(b)?;
            let mut name = b.display_name(b.kind);
            if !used.insert(name.clone()) {
                let suffix = if b.id.len() >= 8 { &b.id[..8] } else { &b.id };
                name = format!("{name} ({suffix})");
                used.insert(name.clone());
            }
            Some((name, t))
        })
        .collect()
}
