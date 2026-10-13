//! 设置面板：基于 [GPUI Kit Settings](https://gpui-kit.com/zh-CN/component/settings/) 组件。
//!
//! 草稿配置放在 [`SettingsDraft`] Global；字段通过 get/set 闭包读写。
//! 点「保存」才落盘并 `reload_config`。

use std::collections::BTreeMap;
use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::group_box::GroupBoxVariant;
use gpui_kit::component::h_flex;
use gpui_kit::component::setting::{
    NumberFieldOptions, RenderOptions, SettingField, SettingGroup, SettingItem, SettingPage,
    Settings,
};
use gpui_kit::component::v_flex;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, IconName, Sizable as _, Size};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::config::{BackendEntry, BackendKind, Config, HotkeySpec};
use crate::app::paths;
use crate::app::state::AppState;
use crate::services::ocr::{self, OcrBackendKind};
use crate::shared::actions::{CloseWindow, UTILITY_KEY_CONTEXT};
use crate::shared::theme::{LANGUAGES, palette, sync_kit_theme};

/// 设置窗口打开期间的草稿（未保存）。
pub struct SettingsDraft {
    pub cfg: Config,
    pub secret_inputs: BTreeMap<String, String>,
    pub status: Option<String>,
}

impl Global for SettingsDraft {}

fn draft(cx: &App) -> &SettingsDraft {
    cx.global::<SettingsDraft>()
}

fn draft_mut(cx: &mut App) -> &mut SettingsDraft {
    cx.global_mut::<SettingsDraft>()
}

#[derive(Clone)]
pub struct SettingsActions {
    pub on_close: Arc<dyn Fn() + Send + Sync>,
    pub on_save: Arc<dyn Fn(Config) + Send + Sync>,
    pub on_toggle_backend: Arc<dyn Fn(String) + Send + Sync>,
    pub on_clear_history: Arc<dyn Fn() + Send + Sync>,
    pub on_secret_input: Arc<dyn Fn(String) + Send + Sync>,
}

impl Default for SettingsActions {
    fn default() -> Self {
        Self {
            on_close: Arc::new(|| {}),
            on_save: Arc::new(|_| {}),
            on_toggle_backend: Arc::new(|_| {}),
            on_clear_history: Arc::new(|| {}),
            on_secret_input: Arc::new(|_| {}),
        }
    }
}

pub struct SettingsView {
    pub actions: SettingsActions,
    focus: FocusHandle,
    _appearance: Subscription,
}

impl SettingsView {
    pub fn new(
        cfg: Config,
        actions: SettingsActions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Settings 组件读 Kit Theme；与窗口外观对齐，避免顶栏深色、内容浅色。
        sync_kit_theme(Some(window), cx);
        cx.set_global(SettingsDraft {
            cfg,
            secret_inputs: BTreeMap::new(),
            status: None,
        });
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            sync_kit_theme(Some(window), cx);
            cx.notify();
        });
        let focus = cx.focus_handle().tab_stop(true);
        focus.focus(window, cx);
        Self {
            actions,
            focus,
            _appearance: appearance,
        }
    }

    fn close(&mut self, _: &CloseWindow, window: &mut Window, _cx: &mut Context<Self>) {
        (self.actions.on_close)();
        window.remove_window();
    }

    fn build_pages(&self, view: Entity<Self>, cx: &App) -> Vec<SettingPage> {
        vec![
            self.page_general(view.clone()),
            self.page_hotkeys(view.clone()),
            self.page_backends(view.clone(), cx),
            self.page_about(),
        ]
    }

    fn page_general(&self, view: Entity<Self>) -> SettingPage {
        let lang_opts: Vec<(SharedString, SharedString)> = LANGUAGES
            .iter()
            .filter(|(c, _, _)| *c != "auto")
            .map(|(code, _, label)| ((*code).into(), (*label).into()))
            .collect();

        let ocr_opts: Vec<(SharedString, SharedString)> = OcrBackendKind::available()
            .iter()
            .map(|k| (k.as_str().into(), k.label().into()))
            .collect();

        SettingPage::new("常规")
            .icon(IconName::Settings2)
            .default_open(true)
            .resettable(false)
            .groups(vec![
                SettingGroup::new().title("翻译").items(vec![
                    SettingItem::new(
                        "目标语言",
                        SettingField::scrollable_dropdown(
                            lang_opts,
                            |cx| draft(cx).cfg.target_lang.clone().into(),
                            |val, cx| {
                                draft_mut(cx).cfg.target_lang = val.to_string();
                                draft_mut(cx).status =
                                    Some(format!("目标语言：{val}（保存后生效）"));
                            },
                        ),
                    )
                    .description("检测到源 = 目标时，可配合「自动交换」反向翻译。"),
                    SettingItem::new(
                        "自动交换",
                        SettingField::switch(
                            |cx| draft(cx).cfg.auto_swap,
                            |val, cx| draft_mut(cx).cfg.auto_swap = val,
                        )
                        .default_value(true),
                    )
                    .description("源语言与目标语言相同时自动对调。"),
                ]),
                SettingGroup::new().title("弹窗").items(vec![
                    SettingItem::new(
                        "默认固定弹窗",
                        SettingField::switch(
                            |cx| draft(cx).cfg.pin_popup_by_default,
                            |val, cx| draft_mut(cx).cfg.pin_popup_by_default = val,
                        )
                        .default_value(false),
                    )
                    .description("打开翻译窗时默认钉住，失焦不关闭。"),
                    SettingItem::new(
                        "弹窗跟随鼠标",
                        SettingField::switch(
                            |cx| draft(cx).cfg.popup_follow_mouse,
                            |val, cx| draft_mut(cx).cfg.popup_follow_mouse = val,
                        )
                        .default_value(false),
                    )
                    .description("关闭时出现在屏幕中上方。"),
                ]),
                SettingGroup::new()
                    .title("OCR")
                    .items(vec![
                        SettingItem::new(
                            "OCR 引擎",
                            SettingField::dropdown(
                                ocr_opts,
                                |cx| draft(cx).cfg.ocr_backend.clone().into(),
                                {
                                    let view = view.clone();
                                    move |val, cx| {
                                        let kind = OcrBackendKind::parse(val.as_str());
                                        if kind.is_cloud_placeholder() {
                                            draft_mut(cx).status =
                                                Some(format!("{}：口子已预留，尚未接入", kind.label()));
                                        } else {
                                            draft_mut(cx).cfg.ocr_backend = kind.as_str().into();
                                            draft_mut(cx).status =
                                                Some(format!("OCR：{}（保存后生效）", kind.label()));
                                        }
                                        view.update(cx, |_, cx| cx.notify());
                                    }
                                },
                            ),
                        ),
                        SettingItem::new(
                            "连续识别",
                            SettingField::switch(
                                |cx| draft(cx).cfg.ocr_continuous,
                                |val, cx| draft_mut(cx).cfg.ocr_continuous = val,
                            )
                            .default_value(false),
                        )
                        .description("新结果追加到 OCR 窗口。"),
                        SettingItem::new(
                            "自动复制",
                            SettingField::switch(
                                |cx| draft(cx).cfg.ocr_auto_copy,
                                |val, cx| draft_mut(cx).cfg.ocr_auto_copy = val,
                            )
                            .default_value(true),
                        ),
                        SettingItem::new(
                            "智能分段",
                            SettingField::switch(
                                |cx| draft(cx).cfg.ocr_smart_paragraph,
                                |val, cx| draft_mut(cx).cfg.ocr_smart_paragraph = val,
                            )
                            .default_value(true),
                        )
                        .description("按段落合并断行。"),
                        SettingItem::render({
                            let view = view.clone();
                            move |options, _, cx| ort_download_row(options, view.clone(), cx)
                        })
                        .keywords(["ONNX", "OCR", "下载", "引擎"]),
                    ])
                    .footer(|_, _| {
                        "本地 RapidOCR 需先下载 ONNX Runtime；模型在首次识别时自动缓存。云端 OCR 口子已预留。"
                    }),
                SettingGroup::new().title("历史").item(
                    SettingItem::new(
                        "历史最大条数",
                        SettingField::number_input(
                            NumberFieldOptions {
                                min: 0.0,
                                max: 10_000.0,
                                ..Default::default()
                            },
                            |cx| draft(cx).cfg.history_limit as f64,
                            |val, cx| {
                                draft_mut(cx).cfg.history_limit = val.max(0.0) as usize;
                            },
                        )
                        .default_value(200.0),
                    )
                    .description("0 表示不限制。"),
                ),
            ])
    }

    fn page_hotkeys(&self, view: Entity<Self>) -> SettingPage {
        type HotkeyGetter = fn(&Config) -> &HotkeySpec;
        let items: Vec<(&str, HotkeyGetter)> = vec![
            ("划词翻译", |c| &c.translate_hotkey),
            ("截图翻译", |c| &c.ocr_hotkey),
            ("截图 OCR", |c| &c.ocr_only_hotkey),
            ("静默截图 OCR", |c| &c.ocr_silent_hotkey),
            (
                if cfg!(target_os = "macos") {
                    "访达选图 OCR"
                } else if cfg!(target_os = "windows") {
                    "资源管理器选图 OCR"
                } else {
                    "选中文件 OCR"
                },
                |c| &c.ocr_finder_hotkey,
            ),
            ("静默划词替换", |c| &c.silent_replace_hotkey),
            ("静默输入框替换", |c| &c.silent_input_hotkey),
            ("原图翻译", |c| &c.overlay_hotkey),
            ("收藏夹", |c| &c.favorites_hotkey),
            ("输入翻译窗口", |c| &c.input_hotkey),
            ("朗读选中文本", |c| &c.speak_hotkey),
            ("切换弹窗固定", |c| &c.pin_hotkey),
            ("显示历史", |c| &c.history_hotkey),
            ("显示设置", |c| &c.settings_hotkey),
        ];

        let mut hotkey_items: Vec<SettingItem> = items
            .into_iter()
            .map(|(label, getter)| {
                SettingItem::new(
                    label,
                    SettingField::input(
                        move |cx| SharedString::from(getter(&draft(cx).cfg).to_string()),
                        |_, cx| {
                            draft_mut(cx).status = Some("快捷键重绑定将在后续版本中提供".into());
                        },
                    ),
                )
                .description("当前为只读展示；重绑定将在后续版本提供。")
            })
            .collect();

        let clear = self.actions.on_clear_history.clone();
        hotkey_items.push(
            SettingItem::render({
                let view = view.clone();
                move |options, _, _cx| {
                    Button::new("clear_hist")
                        .danger()
                        .label("清空历史")
                        .with_size(options.size())
                        .on_click({
                            let clear = clear.clone();
                            let view = view.clone();
                            move |_, _, cx| {
                                clear();
                                draft_mut(cx).status = Some("历史已清空".into());
                                view.update(cx, |_, cx| cx.notify());
                            }
                        })
                }
            })
            .keywords(["清空", "历史", "数据"]),
        );

        SettingPage::new("快捷键")
            .icon(IconName::BookOpen)
            .resettable(false)
            .group(
                SettingGroup::new()
                    .title("全局热键")
                    .items(hotkey_items)
                    .footer(|_, _| "修改热键后需保存；重绑定 UI 尚未开放。"),
            )
    }

    fn page_backends(&self, view: Entity<Self>, cx: &App) -> SettingPage {
        let mut openai_items: Vec<SettingItem> = Vec::new();
        let mut other_items: Vec<SettingItem> = Vec::new();

        let backends = draft(cx).cfg.backends.clone();
        for b in backends {
            if b.kind == BackendKind::OpenAiCompat {
                openai_items.push(openai_backend_item(b, view.clone(), self.actions.clone()));
            } else {
                other_items.push(builtin_backend_item(b, view.clone(), self.actions.clone()));
            }
        }

        openai_items.push(SettingItem::render({
            let view = view.clone();
            move |options, _, _| {
                Button::new("add_openai")
                    .primary()
                    .label("+ 添加 OpenAI 兼容接口")
                    .with_size(options.size())
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            let n = draft(cx)
                                .cfg
                                .backends
                                .iter()
                                .filter(|b| b.kind == BackendKind::OpenAiCompat)
                                .count()
                                + 1;
                            let mut params = BTreeMap::new();
                            params.insert("base".into(), "https://api.openai.com/v1".into());
                            params.insert("model".into(), "gpt-4o-mini".into());
                            draft_mut(cx).cfg.backends.insert(
                                0,
                                BackendEntry::openai_compat(
                                    format!("OpenAI 兼容 {n}"),
                                    "https://api.openai.com/v1",
                                    params,
                                    true,
                                ),
                            );
                            draft_mut(cx).status =
                                Some("已添加接口，请填写地址与密钥后保存".into());
                            view.update(cx, |_, cx| cx.notify());
                        }
                    })
            }
        }));

        other_items.push(SettingItem::render({
            let view = view.clone();
            move |options, _, _| {
                Button::new("verify_backends")
                    .primary()
                    .label("验证已启用接口")
                    .with_size(options.size())
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            draft_mut(cx).status = Some("正在验证…".into());
                            view.update(cx, |_, cx| cx.notify());
                            let cfg = draft(cx).cfg.clone();
                            let view2 = view.clone();
                            cx.spawn(async move |cx| {
                                let msg = cx
                                    .background_executor()
                                    .spawn(async move { probe_backends(&cfg) })
                                    .await;
                                view2.update(cx, |_, cx| {
                                    draft_mut(cx).status = Some(msg);
                                    cx.notify();
                                });
                            })
                            .detach();
                        }
                    })
            }
        }));

        let tts_opts = vec![
            ("auto".into(), "系统 (auto)".into()),
            ("system".into(), "系统".into()),
            ("microsoft".into(), "Microsoft".into()),
            ("google".into(), "Google".into()),
        ];

        SettingPage::new("翻译接口")
            .icon(IconName::Globe)
            .resettable(false)
            .groups(vec![
                SettingGroup::new()
                    .title("OpenAI 兼容（可多条）")
                    .items(openai_items)
                    .footer(|_, _| {
                        "自定义名称 / 接口地址 / 模型 / API Key；响应以流式增量展示。点字段从剪贴板写入。"
                    }),
                SettingGroup::new()
                    .title("其它翻译接口")
                    .items(other_items)
                    .footer(|_, _| "勾选「已启用」才会调用；启用多个时弹窗里并排对比。"),
                SettingGroup::new()
                    .title("语音合成")
                    .item(
                        SettingItem::new(
                            "TTS 后端",
                            SettingField::dropdown(
                                tts_opts,
                                |cx| draft(cx).cfg.tts_backend.clone().into(),
                                |val, cx| draft_mut(cx).cfg.tts_backend = val.to_string(),
                            ),
                        )
                        .description(
                            "云 TTS 需配置 MICROSOFT_TTS_KEY / GOOGLE_TTS_API_KEY 或写入钥匙串。",
                        ),
                    ),
            ])
    }

    fn page_about(&self) -> SettingPage {
        let version = env!("CARGO_PKG_VERSION").to_string();
        let config_path = paths::config_file().display().to_string();
        let history_path = paths::history_file().display().to_string();

        SettingPage::new("关于")
            .icon(IconName::Info)
            .resettable(false)
            .group(SettingGroup::new().items(vec![
                SettingItem::render(move |_, _, cx| {
                    let muted = cx.theme().muted_foreground;
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(cx.theme().foreground)
                                .child(format!("Transel {version}")),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child("基于 GPUI + gpui-kit（macOS / Windows / Linux）"),
                        )
                }),
                SettingItem::new(
                    "配置文件",
                    SettingField::input(
                        {
                            let p = config_path.clone();
                            move |_| SharedString::from(p.clone())
                        },
                        |_, _| {},
                    ),
                )
                .layout(Axis::Vertical),
                SettingItem::new(
                    "历史文件",
                    SettingField::input(
                        {
                            let p = history_path.clone();
                            move |_| SharedString::from(p.clone())
                        },
                        |_, _| {},
                    ),
                )
                .layout(Axis::Vertical),
            ]))
    }
}

fn ort_download_row(options: &RenderOptions, view: Entity<SettingsView>, cx: &App) -> AnyElement {
    let label = ocr::ort_status_label();
    let st = ocr::ort_status();
    let can_download = matches!(st, ocr::OrtStatus::Missing | ocr::OrtStatus::Downloaded);
    let btn_label = match st {
        ocr::OrtStatus::Ready => "已就绪",
        ocr::OrtStatus::Downloaded => "加载引擎",
        ocr::OrtStatus::Missing => "下载本地 OCR 引擎",
        ocr::OrtStatus::Unsupported => "当前平台不可用",
    };
    let size = options.size();

    h_flex()
        .w_full()
        .items_center()
        .gap_2()
        .child(
            Button::new("ocr_download_ort")
                .when(can_download, |b| b.primary())
                .disabled(!can_download)
                .label(btn_label)
                .with_size(size)
                .on_click({
                    let view = view.clone();
                    move |_, _, cx| {
                        if !can_download {
                            return;
                        }
                        draft_mut(cx).status = Some("正在下载 ONNX Runtime…".into());
                        view.update(cx, |_, cx| cx.notify());
                        let view2 = view.clone();
                        cx.spawn(async move |cx| {
                            let result = cx
                                .background_executor()
                                .spawn(async {
                                    ocr::download_ort_runtime().map(|p| p.display().to_string())
                                })
                                .await;
                            view2.update(cx, |_, cx| {
                                match result {
                                    Ok(path) => {
                                        let backend = draft(cx).cfg.ocr_backend.clone();
                                        std::thread::Builder::new()
                                            .name("ocr-warmup".into())
                                            .spawn(move || ocr::set_engine_from_config(&backend))
                                            .ok();
                                        draft_mut(cx).status =
                                            Some(format!("ONNX Runtime 已就绪\n{path}"));
                                    }
                                    Err(e) => {
                                        draft_mut(cx).status = Some(format!("下载失败: {e:#}"));
                                    }
                                }
                                cx.notify();
                            });
                        })
                        .detach();
                    }
                }),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!(
                    "状态：{label} · Microsoft ONNX Runtime {}（约数十 MB）",
                    ocr::ORT_VERSION
                )),
        )
        .into_any_element()
}

fn openai_backend_item(
    b: BackendEntry,
    view: Entity<SettingsView>,
    actions: SettingsActions,
) -> SettingItem {
    let id = b.id.clone();
    let id_kw = id.clone();
    SettingItem::render(move |options, window, cx| {
        let draft = draft(cx);
        let entry = draft
            .cfg
            .backends
            .iter()
            .find(|x| x.id == id)
            .cloned()
            .unwrap_or_else(|| b.clone());
        let enabled = entry.enabled;
        let title = if entry.display_name.is_empty() {
            kind_label(&entry.kind).to_string()
        } else {
            entry.display_name.clone()
        };
        let base = entry.params.get("base").cloned().unwrap_or_default();
        let model = entry
            .params
            .get("model")
            .cloned()
            .unwrap_or_else(|| "gpt-4o-mini".into());
        let key_hint = draft.secret_inputs.get(&id).cloned().unwrap_or_default();
        let has_key_ref = entry
            .params
            .get("key_ref")
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let p = palette(window.appearance());

        v_flex()
            .w_full()
            .gap_1()
            .p_2()
            .rounded(px(8.))
            .bg(p.tab_inactive)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(p.muted)
                            .child("openai_compat · 流式"),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new(SharedString::from(format!("be_del_{id}")))
                            .danger()
                            .xsmall()
                            .label("删除")
                            .with_size(options.size())
                            .on_click({
                                let id = id.clone();
                                let view = view.clone();
                                move |_, _, cx| {
                                    draft_mut(cx).cfg.backends.retain(|b| b.id != id);
                                    draft_mut(cx).secret_inputs.remove(&id);
                                    draft_mut(cx).status = Some("已删除该接口，请点保存".into());
                                    view.update(cx, |_, cx| cx.notify());
                                }
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("be_toggle_{id}")))
                            .when(enabled, |b| b.primary())
                            .xsmall()
                            .label(if enabled { "已启用" } else { "已禁用" })
                            .with_size(options.size())
                            .on_click({
                                let id = id.clone();
                                let view = view.clone();
                                let on_toggle = actions.on_toggle_backend.clone();
                                move |_, _, cx| {
                                    if let Some(b) =
                                        draft_mut(cx).cfg.backends.iter_mut().find(|b| b.id == id)
                                    {
                                        b.enabled = !b.enabled;
                                    }
                                    on_toggle(id.clone());
                                    view.update(cx, |_, cx| cx.notify());
                                }
                            }),
                    ),
            )
            .child(paste_field_row(
                format!("be_name_{id}"),
                "名称",
                entry.display_name.clone(),
                "点击：从剪贴板写入名称",
                &p,
                id.clone(),
                "display_name",
                view.clone(),
            ))
            .child(paste_field_row(
                format!("be_base_{id}"),
                "接口地址",
                base,
                "点击：写入 base URL",
                &p,
                id.clone(),
                "base",
                view.clone(),
            ))
            .child(paste_field_row(
                format!("be_model_{id}"),
                "模型",
                model,
                "点击：写入模型名",
                &p,
                id.clone(),
                "model",
                view.clone(),
            ))
            .child(secret_key_row(
                id.clone(),
                key_hint,
                has_key_ref,
                &p,
                view.clone(),
                actions.on_secret_input.clone(),
                true,
            ))
    })
    .keywords(["OpenAI", "API", "流式", id_kw.as_str()])
}

fn builtin_backend_item(
    b: BackendEntry,
    view: Entity<SettingsView>,
    actions: SettingsActions,
) -> SettingItem {
    let id = b.id.clone();
    let kind = b.kind;
    SettingItem::render(move |options, window, cx| {
        let draft = draft(cx);
        let entry = draft
            .cfg
            .backends
            .iter()
            .find(|x| x.id == id)
            .cloned()
            .unwrap_or_else(|| b.clone());
        let enabled = entry.enabled;
        let has_key = kind.needs_key();
        let val = draft.secret_inputs.get(&id).cloned().unwrap_or_default();
        let has_key_ref = entry
            .params
            .get("key_ref")
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let p = palette(window.appearance());

        v_flex()
            .w_full()
            .gap_1()
            .p_2()
            .rounded(px(8.))
            .bg(p.tab_inactive)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(kind_label(&kind)),
                    )
                    .child(div().text_xs().text_color(p.muted).child(kind.as_str()))
                    .child(div().flex_1())
                    .child(
                        Button::new(SharedString::from(format!("be_toggle_{id}")))
                            .when(enabled, |b| b.primary())
                            .xsmall()
                            .label(if enabled { "已启用" } else { "已禁用" })
                            .with_size(options.size())
                            .on_click({
                                let id = id.clone();
                                let view = view.clone();
                                let on_toggle = actions.on_toggle_backend.clone();
                                move |_, _, cx| {
                                    if let Some(b) =
                                        draft_mut(cx).cfg.backends.iter_mut().find(|b| b.id == id)
                                    {
                                        b.enabled = !b.enabled;
                                    }
                                    on_toggle(id.clone());
                                    view.update(cx, |_, cx| cx.notify());
                                }
                            }),
                    ),
            )
            .when(has_key, |el| {
                el.child(secret_key_row(
                    id.clone(),
                    val,
                    has_key_ref,
                    &p,
                    view.clone(),
                    actions.on_secret_input.clone(),
                    false,
                ))
            })
    })
    .keywords([kind_label(&kind), kind.as_str(), "API"])
}

#[allow(clippy::too_many_arguments)]
fn paste_field_row(
    field_id: String,
    label: &'static str,
    value: String,
    placeholder: &'static str,
    p: &crate::shared::theme::Palette,
    entry_id: String,
    param_key: &'static str,
    view: Entity<SettingsView>,
) -> AnyElement {
    let empty = value.is_empty();
    h_flex()
        .w_full()
        .items_center()
        .gap_2()
        .child(div().text_xs().text_color(p.muted).w(px(72.)).child(label))
        .child(
            div()
                .id(SharedString::from(field_id))
                .h(px(26.))
                .rounded(px(5.))
                .bg(p.input_bg)
                .border_1()
                .border_color(p.border)
                .px_2()
                .flex_1()
                .flex()
                .items_center()
                .text_xs()
                .text_color(if empty { p.muted } else { p.text })
                .cursor_pointer()
                .child(if empty {
                    placeholder.to_string()
                } else {
                    value
                })
                .on_click(move |_, _, cx| match read_clipboard_trim() {
                    Ok(v) => {
                        if let Some(b) = draft_mut(cx)
                            .cfg
                            .backends
                            .iter_mut()
                            .find(|b| b.id == entry_id)
                        {
                            match param_key {
                                "display_name" => b.display_name = v.clone(),
                                key => {
                                    b.params.insert(key.into(), v.clone());
                                }
                            }
                            draft_mut(cx).status = Some(format!("{label} 已更新，请点保存"));
                        }
                        view.update(cx, |_, cx| cx.notify());
                    }
                    Err(e) => {
                        draft_mut(cx).status = Some(e);
                        view.update(cx, |_, cx| cx.notify());
                    }
                }),
        )
        .into_any_element()
}

fn secret_key_row(
    id: String,
    key_hint: String,
    has_key_ref: bool,
    p: &crate::shared::theme::Palette,
    view: Entity<SettingsView>,
    on_secret: Arc<dyn Fn(String) + Send + Sync>,
    openai: bool,
) -> AnyElement {
    h_flex()
        .w_full()
        .items_center()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(p.muted)
                .w(px(72.))
                .child("API Key"),
        )
        .child(
            div()
                .id(SharedString::from(format!("be_key_{id}")))
                .h(px(26.))
                .rounded(px(5.))
                .bg(p.input_bg)
                .border_1()
                .border_color(p.border)
                .px_2()
                .flex_1()
                .flex()
                .items_center()
                .text_xs()
                .text_color(if key_hint.is_empty() && !has_key_ref {
                    p.muted
                } else {
                    p.text
                })
                .cursor_pointer()
                .child(if !key_hint.is_empty() {
                    "*".repeat(key_hint.len().min(20))
                } else if has_key_ref {
                    "已配置（点击可覆盖）".to_string()
                } else {
                    "点击：从剪贴板写入钥匙串".to_string()
                })
                .on_click(move |_, _, cx| {
                    let key = match read_clipboard_trim() {
                        Ok(k) => k,
                        Err(e) => {
                            draft_mut(cx).status = Some(e);
                            view.update(cx, |_, cx| cx.notify());
                            return;
                        }
                    };
                    let Some(b) = draft_mut(cx).cfg.backends.iter_mut().find(|b| b.id == id) else {
                        return;
                    };
                    let account = b.secret_account();
                    let store = crate::app::secrets::KeyringStore;
                    if let Err(e) = crate::app::secrets::SecretStore::set(&store, &account, &key) {
                        draft_mut(cx).status = Some(format!("钥匙串写入失败: {e}"));
                        view.update(cx, |_, cx| cx.notify());
                        return;
                    }
                    b.params
                        .insert("key_ref".into(), crate::app::secrets::make_ref(&account));
                    let label = if openai {
                        if b.display_name.is_empty() {
                            kind_label(&b.kind).to_string()
                        } else {
                            b.display_name.clone()
                        }
                    } else {
                        kind_label(&b.kind).to_string()
                    };
                    draft_mut(cx).secret_inputs.insert(id.clone(), key);
                    draft_mut(cx).status = Some(format!("{label} 密钥已写入，请点保存"));
                    on_secret(id.clone());
                    view.update(cx, |_, cx| cx.notify());
                }),
        )
        .into_any_element()
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 外壳与翻译弹窗同一套 Palette；Kit Settings 色已由 sync_kit_theme 对齐。
        let p = palette(window.appearance());
        let view = cx.entity();
        let pages = self.build_pages(view.clone(), cx);
        let status = if cx.has_global::<SettingsDraft>() {
            draft(cx).status.clone().unwrap_or_default()
        } else {
            String::new()
        };

        let topbar = h_flex()
            .w_full()
            .items_center()
            .h(px(40.))
            .px(px(14.))
            .gap_2()
            .border_b_1()
            .border_color(p.divider)
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.title)
                    .child("Transel · 设置"),
            )
            .child(div().flex_1())
            .child(div().text_xs().text_color(p.muted).child(status))
            .child(
                Button::new("save")
                    .primary()
                    .label("保存")
                    .with_size(Size::Small)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !cx.has_global::<SettingsDraft>() {
                            return;
                        }
                        let draft_cfg = draft(cx).cfg.clone();
                        if let Err(e) = draft_cfg.save(&paths::config_file()) {
                            draft_mut(cx).status = Some(format!("保存失败: {e}"));
                            cx.notify();
                            return;
                        }
                        let backend = draft_cfg.ocr_backend.clone();
                        std::thread::Builder::new()
                            .name("ocr-switch".into())
                            .spawn(move || ocr::set_engine_from_config(&backend))
                            .ok();
                        cx.global::<AppState>().reload_config(draft_cfg.clone());
                        (this.actions.on_save)(draft_cfg);
                        draft_mut(cx).status = Some("已保存并热更新".into());
                        cx.notify();
                    })),
            )
            .child(
                Button::new("settings_close")
                    .ghost()
                    .label("关闭")
                    .with_size(Size::Small)
                    .on_click(cx.listener(|this, _, window, _cx| {
                        (this.actions.on_close)();
                        window.remove_window();
                    })),
            );

        // 侧栏 / 搜索区：贴近旧设置页左导航（浅底 + 右边框），与弹窗色板一致。
        let sidebar_style = StyleRefinement::default()
            .bg(transparent_black())
            .border_r_1()
            .border_color(p.divider)
            .text_color(p.text);
        let header_style = StyleRefinement::default().p(px(10.));

        let body = Settings::new("transel-settings")
            .with_size(Size::Small)
            .with_group_variant(GroupBoxVariant::Fill)
            .sidebar_width(px(160.))
            .sidebar_style(&sidebar_style)
            .header_style(&header_style)
            .pages(pages);

        let card = v_flex()
            .id("settings_card")
            .size_full()
            .bg(p.card)
            .border_1()
            .border_color(p.border)
            .rounded(px(crate::platform::CORNER_RADIUS as f32))
            .font_family(".SystemUIFont")
            .when(!cfg!(target_os = "macos"), |el| {
                el.shadow(vec![BoxShadow {
                    color: p.shadow,
                    offset: point(px(0.), px(4.)),
                    blur_radius: px(10.),
                    spread_radius: px(0.),
                    inset: false,
                }])
            })
            .child(topbar)
            .child(
                div()
                    .id("settings_body")
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .child(body),
            );

        div()
            .id("settings_root")
            .key_context(UTILITY_KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close))
            .size_full()
            .p(px(crate::shared::tokens::SHADOW_MARGIN))
            .child(card)
    }
}

fn kind_label(k: &BackendKind) -> &'static str {
    k.default_display_name()
}

fn read_clipboard_trim() -> Result<String, String> {
    let mut cb = arboard::Clipboard::new().map_err(|_| "无法读取剪贴板".to_string())?;
    let raw = cb.get_text().map_err(|_| "剪贴板无文本".to_string())?;
    let key = raw.trim().to_string();
    if key.is_empty() {
        return Err("剪贴板为空".into());
    }
    Ok(key)
}

/// 对已启用后端发一次探测翻译。
fn probe_backends(cfg: &Config) -> String {
    use crate::services::translate::{RequestCtx, TranslateService};
    let svc = TranslateService::start(cfg);
    let enabled: Vec<_> = cfg.backends.iter().filter(|b| b.enabled).collect();
    if enabled.is_empty() {
        return "没有已启用的翻译接口".into();
    }
    let rx = svc.request(RequestCtx {
        source: "hello".into(),
        prepared: "hello".into(),
        source_lang_hint: Some("en".into()),
        target_lang: cfg.target_lang.clone(),
        only: None,
    });
    match futures::executor::block_on(crate::services::translate::collect_batch(rx)) {
        Ok(batch) => {
            let ok: Vec<_> = batch
                .results
                .iter()
                .filter(|r| r.ok && !r.pending)
                .map(|r| r.backend.clone())
                .collect();
            let fail: Vec<_> = batch
                .results
                .iter()
                .filter(|r| !r.ok && !r.pending)
                .map(|r| {
                    format!(
                        "{}({})",
                        r.backend,
                        r.error.clone().unwrap_or_else(|| "失败".into())
                    )
                })
                .collect();
            if fail.is_empty() {
                format!("验证成功: {}", ok.join(", "))
            } else if ok.is_empty() {
                format!("全部失败: {}", fail.join("; "))
            } else {
                format!("部分成功: {} | 失败: {}", ok.join(", "), fail.join("; "))
            }
        }
        Err(e) => format!("验证失败: {e}"),
    }
}
