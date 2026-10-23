//! Windows 截图：GDI 全屏/区域；交互框选走 WinForms 覆盖层。

use std::path::PathBuf;
use std::process::Command;

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

use crate::app::paths;
use crate::services::ocr::platform::{ImageData, new_png_path, read_png};

pub(super) fn platform_capture_full_screen() -> anyhow::Result<(ImageData, PathBuf)> {
    let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let w = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let h = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    capture_rect(x, y, w as u32, h as u32, "full")
}

pub(super) fn platform_capture_region(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
) -> anyhow::Result<(ImageData, PathBuf)> {
    if w > 1 && h > 1 {
        return capture_rect(x, y, w, h, "region");
    }
    let path = capture_interactive_path()?;
    let img = read_png(&path)?;
    Ok((img, path))
}

pub(super) fn capture_interactive_path() -> anyhow::Result<PathBuf> {
    capture_interactive_custom(false)
}

pub(super) fn capture_interactive_annotated_path() -> anyhow::Result<PathBuf> {
    capture_interactive_custom(true)
}

fn capture_interactive_custom(_annotate: bool) -> anyhow::Result<PathBuf> {
    let path = new_png_path("region");
    let out_path = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("截图路径非 UTF-8"))?;
    let escaped = out_path.replace('\'', "''");
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$out = '{escaped}'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Drawing;
using System.Windows.Forms;
public class TranselSnip : Form {{
  Point start; Rectangle rect; bool dragging; public string OutPath;
  public TranselSnip(string path) {{
    OutPath = path;
    FormBorderStyle = FormBorderStyle.None;
    StartPosition = FormStartPosition.Manual;
    Bounds = SystemInformation.VirtualScreen;
    TopMost = true;
    Opacity = 0.32;
    BackColor = Color.Black;
    Cursor = Cursors.Cross;
    DoubleBuffered = true;
    KeyPreview = true;
    ShowInTaskbar = false;
    KeyDown += (s,e) => {{ if (e.KeyCode == Keys.Escape) {{ DialogResult = DialogResult.Cancel; Close(); }} }};
    MouseDown += (s,e) => {{ if (e.Button == MouseButtons.Left) {{ dragging = true; start = PointToScreen(e.Location); rect = Rectangle.Empty; }} }};
    MouseMove += (s,e) => {{
      if (!dragging) return;
      var p = PointToScreen(e.Location);
      rect = Rect(start, p); Invalidate();
    }};
    MouseUp += (s,e) => {{
      if (!dragging) return;
      dragging = false;
      var p = PointToScreen(e.Location);
      rect = Rect(start, p);
      if (rect.Width < 4 || rect.Height < 4) {{ DialogResult = DialogResult.Cancel; Close(); return; }}
      Hide(); Opacity = 0;
      using (var bmp = new Bitmap(rect.Width, rect.Height))
      using (var g = Graphics.FromImage(bmp)) {{
        g.CopyFromScreen(rect.Location, Point.Empty, rect.Size);
        bmp.Save(OutPath, System.Drawing.Imaging.ImageFormat.Png);
      }}
      DialogResult = DialogResult.OK; Close();
    }};
  }}
  static Rectangle Rect(Point a, Point b) {{
    return new Rectangle(Math.Min(a.X,b.X), Math.Min(a.Y,b.Y), Math.Abs(a.X-b.X), Math.Abs(a.Y-b.Y));
  }}
  protected override void OnPaint(PaintEventArgs e) {{
    if (rect.Width <= 0) return;
    var local = RectangleToClient(rect);
    using (var pBlue = new Pen(Color.FromArgb(0, 122, 255), 2)) e.Graphics.DrawRectangle(pBlue, local);
    using (var pWhite = new Pen(Color.FromArgb(80, 255, 255, 255), 1)) {{
      var inner = new Rectangle(local.X + 1, local.Y + 1, Math.Max(0, local.Width - 2), Math.Max(0, local.Height - 2));
      e.Graphics.DrawRectangle(pWhite, inner);
    }}
    string badge = $"{rect.Width} x {rect.Height}";
    using (var font = new Font("Segoe UI", 9, FontStyle.Bold))
    using (var bgBrush = new SolidBrush(Color.FromArgb(220, 20, 20, 20)))
    using (var textBrush = new SolidBrush(Color.White)) {{
      var sz = e.Graphics.MeasureString(badge, font);
      float bx = Math.Max(10, local.X);
      float by = local.Y - sz.Height - 6;
      if (by < 10) by = local.Y + 6;
      e.Graphics.FillRectangle(bgBrush, bx, by, sz.Width + 12, sz.Height + 4);
      e.Graphics.DrawString(badge, font, textBrush, bx + 6, by + 2);
    }}
  }}
}}
"@
$form = New-Object TranselSnip $out
$r = $form.ShowDialog()
if ($r -ne [System.Windows.Forms.DialogResult]::OK) {{ exit 1 }}
"#
    );
    let ps1 = paths::ocr_cache_dir().join("st-snip.ps1");
    std::fs::write(&ps1, script)?;
    let out = Command::new("powershell")
        .args(["-NoProfile", "-STA", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&ps1)
        .output()?;
    if !out.status.success() || !path.exists() {
        let err = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!(
            "已取消截图或截图失败{}",
            if err.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", err.trim())
            }
        );
    }
    Ok(path)
}

fn capture_rect(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    prefix: &str,
) -> anyhow::Result<(ImageData, PathBuf)> {
    let img = gdi_capture(x, y, w, h)?;
    let path = new_png_path(prefix);
    img.write_png(&path)?;
    Ok((img, path))
}

fn gdi_capture(x: i32, y: i32, w: u32, h: u32) -> anyhow::Result<ImageData> {
    if w == 0 || h == 0 {
        anyhow::bail!("无效截图区域");
    }
    let width = w as i32;
    let height = h as i32;
    unsafe {
        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            anyhow::bail!("GetDC 失败");
        }
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        if hdc_mem.is_invalid() {
            ReleaseDC(None, hdc_screen);
            anyhow::bail!("CreateCompatibleDC 失败");
        }
        let hbmp = CreateCompatibleBitmap(hdc_screen, width, height);
        let old = SelectObject(hdc_mem, hbmp.into());
        let blit = BitBlt(
            hdc_mem,
            0,
            0,
            width,
            height,
            Some(hdc_screen),
            x,
            y,
            SRCCOPY,
        );
        if blit.is_err() {
            SelectObject(hdc_mem, old);
            let _ = DeleteObject(hbmp.into());
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(None, hdc_screen);
            anyhow::bail!("BitBlt 失败");
        }

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        let got = GetDIBits(
            hdc_mem,
            hbmp,
            0,
            h,
            Some(buf.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        SelectObject(hdc_mem, old);
        let _ = DeleteObject(hbmp.into());
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(None, hdc_screen);
        if got == 0 {
            anyhow::bail!("GetDIBits 失败");
        }
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2);
            px[3] = 255;
        }
        Ok(ImageData {
            width: w,
            height: h,
            rgba: buf,
        })
    }
}
