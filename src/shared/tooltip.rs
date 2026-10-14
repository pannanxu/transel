//! 与弹窗配色一致的紧凑 Tooltip。

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;

use crate::shared::theme::palette;

/// 构建悬浮提示：实色底、小字号、贴合触发点，避免 Kit 默认的大边距半透明气泡。
pub fn build(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |window, cx| {
        let p = palette(window.appearance());
        let bg = match window.appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => hsla(0., 0., 0.16, 0.96),
            _ => hsla(0., 0., 0.99, 0.97),
        };
        Tooltip::new(text.clone())
            .m(px(4.))
            .px(px(8.))
            .py(px(3.))
            .text_size(px(11.))
            .line_height(px(16.))
            .font_family(".SystemUIFont")
            .font_weight(FontWeight::MEDIUM)
            .bg(bg)
            .text_color(p.text)
            .border_1()
            .border_color(p.border)
            .rounded(px(6.))
            .shadow(vec![BoxShadow {
                color: p.shadow,
                offset: point(px(0.), px(2.)),
                blur_radius: px(8.),
                spread_radius: px(0.),
                inset: false,
            }])
            .build(window, cx)
    }
}
