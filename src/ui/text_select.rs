//! 纯文本可选中展示（拖选 + Cmd/Ctrl+C），不做 Markdown 渲染。

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
