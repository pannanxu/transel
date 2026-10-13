//! macOS：辅助功能 API 读取选中文字 / 整框替换。

use accessibility_sys::{
    AXIsProcessTrusted, AXIsProcessTrustedWithOptions, AXUIElementCopyAttributeValue,
    AXUIElementCreateSystemWide, AXUIElementRef, AXUIElementSetAttributeValue, kAXErrorSuccess,
    kAXFocusedUIElementAttribute, kAXSelectedTextAttribute, kAXTrustedCheckOptionPrompt,
    kAXValueAttribute,
};
use core_foundation::base::{CFGetTypeID, CFRelease, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};

pub fn is_accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

pub fn ensure_accessibility_permission() -> bool {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt as CFStringRef);
        let opts = CFDictionary::from_CFType_pairs(&[(
            key.as_CFType(),
            CFBoolean::true_value().as_CFType(),
        )]);
        AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef() as _)
    }
}

unsafe fn copy_attr(element: AXUIElementRef, name: &str) -> Option<CFTypeRef> {
    let attr = CFString::new(name);
    let mut value: CFTypeRef = std::ptr::null();
    let err = unsafe {
        AXUIElementCopyAttributeValue(
            element,
            attr.as_concrete_TypeRef() as _,
            &mut value as *mut CFTypeRef as *mut _,
        )
    };
    if err == kAXErrorSuccess && !value.is_null() {
        Some(value)
    } else {
        None
    }
}

pub fn selected_text() -> Option<String> {
    if !is_accessibility_trusted() {
        return None;
    }
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return None;
        }
        let focused = copy_attr(system, kAXFocusedUIElementAttribute);
        CFRelease(system as CFTypeRef);
        let focused = focused?;
        let value = copy_attr(focused as AXUIElementRef, kAXSelectedTextAttribute);
        CFRelease(focused);
        let value = value?;
        if CFGetTypeID(value) != CFString::type_id() {
            CFRelease(value);
            return None;
        }
        Some(CFString::wrap_under_create_rule(value as CFStringRef).to_string())
    }
}

/// 焦点控件的完整文本（AXValue）。许多 Electron / Web 控件会失败，调用方应有 Cmd+A/C 兜底。
pub fn focused_value() -> Option<String> {
    if !is_accessibility_trusted() {
        return None;
    }
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return None;
        }
        let focused = copy_attr(system, kAXFocusedUIElementAttribute);
        CFRelease(system as CFTypeRef);
        let focused = focused?;
        let value = copy_attr(focused as AXUIElementRef, kAXValueAttribute);
        CFRelease(focused);
        let value = value?;
        if CFGetTypeID(value) != CFString::type_id() {
            CFRelease(value);
            return None;
        }
        Some(CFString::wrap_under_create_rule(value as CFStringRef).to_string())
    }
}

/// 设置焦点控件文本（AXValue）。成功返回 true。
pub fn set_focused_value(text: &str) -> bool {
    if !is_accessibility_trusted() {
        return false;
    }
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return false;
        }
        let focused = copy_attr(system, kAXFocusedUIElementAttribute);
        CFRelease(system as CFTypeRef);
        let Some(focused) = focused else {
            return false;
        };
        let attr = CFString::new(kAXValueAttribute);
        let val = CFString::new(text);
        let err = AXUIElementSetAttributeValue(
            focused as AXUIElementRef,
            attr.as_concrete_TypeRef() as _,
            val.as_CFTypeRef(),
        );
        CFRelease(focused);
        err == kAXErrorSuccess
    }
}

/// 尝试用 AX 替换选中文本（若控件支持 `AXSelectedText` 可写）。
pub fn set_selected_text(text: &str) -> bool {
    if !is_accessibility_trusted() {
        return false;
    }
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return false;
        }
        let focused = copy_attr(system, kAXFocusedUIElementAttribute);
        CFRelease(system as CFTypeRef);
        let Some(focused) = focused else {
            return false;
        };
        let attr = CFString::new(kAXSelectedTextAttribute);
        let val = CFString::new(text);
        let err = AXUIElementSetAttributeValue(
            focused as AXUIElementRef,
            attr.as_concrete_TypeRef() as _,
            val.as_CFTypeRef(),
        );
        CFRelease(focused);
        err == kAXErrorSuccess
    }
}
