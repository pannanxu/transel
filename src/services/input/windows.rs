//! Windows：UI Automation 读取选中文字 / 焦点框全文。

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationTextPattern, IUIAutomationValuePattern,
    UIA_TextPatternId, UIA_ValuePatternId,
};
use windows::core::BSTR;

fn with_uia<T: Send + 'static>(
    f: impl FnOnce(&IUIAutomation) -> windows::core::Result<T> + Send + 'static,
) -> Option<T> {
    std::thread::spawn(move || unsafe {
        let inited = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let r = (|| {
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            f(&uia)
        })();
        if inited {
            CoUninitialize();
        }
        r.ok()
    })
    .join()
    .ok()
    .flatten()
}

fn uia_selection(uia: &IUIAutomation) -> windows::core::Result<String> {
    unsafe {
        let element = uia.GetFocusedElement()?;
        let pattern: IUIAutomationTextPattern = element.GetCurrentPatternAs(UIA_TextPatternId)?;
        let ranges = pattern.GetSelection()?;
        let mut out = String::new();
        for i in 0..ranges.Length()? {
            let range = ranges.GetElement(i)?;
            out.push_str(&range.GetText(-1)?.to_string());
        }
        Ok(out)
    }
}

pub fn selected_text() -> Option<String> {
    with_uia(|uia| uia_selection(uia)).and_then(
        |s| {
            if s.trim().is_empty() { None } else { Some(s) }
        },
    )
}

/// 焦点控件全文（ValuePattern）。浏览器 / Electron 常失败，调用方应有 Ctrl+A/C 兜底。
pub fn focused_value() -> Option<String> {
    with_uia(|uia| unsafe {
        let element = uia.GetFocusedElement()?;
        let pattern: IUIAutomationValuePattern = element.GetCurrentPatternAs(UIA_ValuePatternId)?;
        Ok(pattern.CurrentValue()?.to_string())
    })
    .and_then(|s| if s.trim().is_empty() { None } else { Some(s) })
}

pub fn set_focused_value(text: &str) -> bool {
    let text = text.to_string();
    with_uia(move |uia| unsafe {
        let element = uia.GetFocusedElement()?;
        let pattern: IUIAutomationValuePattern = element.GetCurrentPatternAs(UIA_ValuePatternId)?;
        pattern.SetValue(&BSTR::from(text.as_str()))
    })
    .is_some()
}
