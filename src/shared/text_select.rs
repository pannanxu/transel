//! 可选中文本展示（拖选 + Cmd/Ctrl+C）。
//!
//! 译文统一走 Markdown [`TextView`]；错误/提示等纯文案仍可用 HTML 转义版。

use gpui_kit::component::text::TextView;
use gpui_kit::*;

/// 将纯文本转成可选中 HTML（转义特殊字符，换行用 `<br/>`）。
pub fn selectable_plain(
    id: impl Into<ElementId>,
    text: &str,
    color: Hsla,
    size: f32,
    line: f32,
) -> AnyElement {
    let mut html = String::with_capacity(text.len() + 16);
    html.push_str("<p>");
    for c in text.chars() {
        match c {
            '&' => html.push_str("&amp;"),
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            '\n' => html.push_str("<br/>"),
            _ => html.push(c),
        }
    }
    html.push_str("</p>");
    TextView::html(id, html)
        .selectable(true)
        .w_full()
        .text_size(px(size))
        .line_height(px(line))
        .text_color(color)
        .into_any_element()
}

/// 列表预览：最多 `lines` 行，超出省略。
pub fn clamped_plain(
    text: impl Into<SharedString>,
    color: Hsla,
    size: f32,
    line: f32,
    lines: usize,
) -> AnyElement {
    let max_h = px(line * lines as f32);
    div()
        .w_full()
        .min_w_0()
        .max_h(max_h)
        .overflow_hidden()
        .text_size(px(size))
        .line_height(px(line))
        .text_color(color)
        .line_clamp(lines)
        .text_ellipsis()
        .child(text.into())
        .into_any_element()
}

/// 译文渲染：一律按 Markdown 解析（见 [TextView](https://gpui-kit.com/zh-CN/component/text-view/)）。
///
/// `stream_fade` 在流式增量更新时启用淡入。
pub fn selectable_markdown(
    id: impl Into<ElementId>,
    text: &str,
    color: Hsla,
    size: f32,
    line: f32,
    stream_fade: bool,
) -> AnyElement {
    TextView::markdown(id, text.to_string())
        .selectable(true)
        .stream_fade(stream_fade)
        .w_full()
        .min_w(px(0.))
        .text_size(px(size))
        .line_height(px(line))
        .text_color(color)
        .into_any_element()
}
