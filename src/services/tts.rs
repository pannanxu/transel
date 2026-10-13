//! 跨平台文本朗读（TTS）。
//!
//! - `auto` / `system`：系统 TTS（macOS say / Windows Speech / Linux spd-say）
//! - `microsoft`：Azure 认知服务 TTS（密钥：`MICROSOFT_TTS_KEY` + `MICROSOFT_TTS_REGION`）
//! - `google`：Google Cloud TTS（密钥：`GOOGLE_TTS_API_KEY`）

use std::process::Command;

use crate::app::secrets::{KeyringStore, SecretStore};

#[derive(Debug, Clone, Copy)]
pub struct TtsOptions<'a> {
    pub text: &'a str,
    /// 语速（-50 ~ +50 等）
    pub rate: Option<i32>,
    /// ISO 语言代码（zh-CN / en-US / ja-JP 等）
    pub language: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct OwnedTtsOptions {
    pub text: String,
    pub rate: Option<i32>,
    pub language: Option<String>,
}

impl OwnedTtsOptions {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            rate: None,
            language: None,
        }
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }
}

/// 后台线程朗读，避免卡住 UI。
pub fn speak_with_backend_async(backend: impl Into<String>, opts: OwnedTtsOptions) {
    let backend = backend.into();
    std::thread::spawn(move || {
        let tts_opts = TtsOptions {
            text: &opts.text,
            rate: opts.rate,
            language: opts.language.as_deref(),
        };
        if let Err(e) = speak_with_backend(&backend, tts_opts) {
            log::warn!("朗读失败: {e:#}");
        }
    });
}

/// 按配置 `tts_backend` 分发；未知值回退系统。
pub fn speak_with_backend(backend: &str, opts: TtsOptions) -> anyhow::Result<()> {
    let text = opts.text.trim();
    if text.is_empty() {
        return Ok(());
    }
    match backend.trim().to_ascii_lowercase().as_str() {
        "microsoft" | "azure" => speak_microsoft(opts),
        "google" | "google_cloud" => speak_google(opts),
        _ => speak_system(opts),
    }
}

pub fn speak(opts: TtsOptions) -> anyhow::Result<()> {
    speak_system(opts)
}

fn speak_system(opts: TtsOptions) -> anyhow::Result<()> {
    let text = opts.text.trim();
    if text.is_empty() {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("/usr/bin/say");
        if let Some(v) = opts.rate {
            cmd.args(["-r", &v.to_string()]);
        }
        if let Some(lang) = opts.language {
            let voice = match lang.to_ascii_lowercase().as_str() {
                "zh" | "zh-cn" | "zh-hans" => "Tingting",
                "zh-tw" | "zh-hant" => "Mei-Jia",
                "en" | "en-us" | "en-gb" => "Samantha",
                "ja" => "Kyoko",
                "ko" => "Yuna",
                "fr" => "Thomas",
                "de" => "Anna",
                "es" => "Monica",
                "ru" => "Milena",
                _ => "Samantha",
            };
            cmd.args(["-v", voice]);
        }
        cmd.arg(text);
        let out = cmd.output()?;
        if !out.status.success() {
            anyhow::bail!("say 失败: {}", String::from_utf8_lossy(&out.stderr));
        }
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        let rate = opts.rate.unwrap_or(0);
        let ps = r#"
Add-Type -AssemblyName System.Speech
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer
$r = [int]$args[1]
if ($r -ne 0) { $s.Rate = [Math]::Max(-10, [Math]::Min(10, $r / 5)) }
if ($args[2]) { try { $s.SelectVoice($args[2]) } catch {} }
$s.Speak($args[0])
$s.Dispose()
"#;
        let voice = match opts.language.unwrap_or("").to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" | "zh-hans" => "Microsoft Huihui Desktop",
            "zh-tw" | "zh-hant" => "Microsoft Hanhan Desktop",
            "en" | "en-us" | "en-gb" => "Microsoft Zira Desktop",
            "ja" => "Microsoft Haruka Desktop",
            "ko" => "Microsoft Heami Desktop",
            _ => "",
        };
        let mut cmd = Command::new("powershell");
        cmd.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            ps,
            text,
            &rate.to_string(),
            voice,
        ]);
        let out = cmd.output()?;
        if !out.status.success() {
            anyhow::bail!(
                "powershell TTS 失败: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        let mut cmd = Command::new("spd-say");
        if let Some(lang) = opts.language {
            cmd.args(["-l", lang]);
        }
        if let Some(r) = opts.rate {
            cmd.args(["-r", &((r + 50) * 2).to_string()]);
        }
        cmd.arg(text);
        if cmd.output().map(|o| o.status.success()).unwrap_or(false) {
            return Ok(());
        }
        for bin in ["espeak-ng", "espeak"] {
            let mut cmd = Command::new(bin);
            if let Some(lang) = opts.language {
                cmd.args(["-v", lang]);
            }
            if let Some(r) = opts.rate {
                cmd.args(["-s", &(175 + r * 3).to_string()]);
            }
            cmd.arg(text);
            if cmd.output().map(|o| o.status.success()).unwrap_or(false) {
                return Ok(());
            }
        }
        anyhow::bail!("朗读失败：spd-say / espeak-ng 都不可用");
    }
    #[allow(unreachable_code)]
    {
        let _ = opts;
        anyhow::bail!("当前平台不支持 TTS");
    }
}

fn speak_microsoft(opts: TtsOptions) -> anyhow::Result<()> {
    let key = std::env::var("MICROSOFT_TTS_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| KeyringStore.get("microsoft_tts_key").ok())
        .ok_or_else(|| anyhow::anyhow!("未配置 Microsoft TTS 密钥"))?;
    let region = std::env::var("MICROSOFT_TTS_REGION")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| KeyringStore.get("microsoft_tts_region").ok())
        .unwrap_or_else(|| "eastasia".into());
    let lang = opts.language.unwrap_or("zh-CN");
    let voice = match lang.to_ascii_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" => ("zh-CN", "zh-CN-XiaoxiaoNeural"),
        "zh-tw" | "zh-hant" => ("zh-TW", "zh-TW-HsiaoChenNeural"),
        "en" | "en-us" => ("en-US", "en-US-JennyNeural"),
        "ja" => ("ja-JP", "ja-JP-NanamiNeural"),
        "ko" => ("ko-KR", "ko-KR-SunHiNeural"),
        _ => ("en-US", "en-US-JennyNeural"),
    };
    let ssml = format!(
        r#"<speak version='1.0' xml:lang='{}'><voice name='{}'>{}</voice></speak>"#,
        voice.0,
        voice.1,
        xml_escape(opts.text)
    );
    let url = format!("https://{region}.tts.speech.microsoft.com/cognitiveservices/v1");
    let bytes = crate::services::translate::ureq_agent()
        .post(url)
        .header("Ocp-Apim-Subscription-Key", &key)
        .header("Content-Type", "application/ssml+xml")
        .header(
            "X-Microsoft-OutputFormat",
            "audio-16khz-128kbitrate-mono-mp3",
        )
        .send(ssml.as_bytes())?
        .body_mut()
        .read_to_vec()?;
    play_audio_bytes(&bytes, "mp3")
}

fn speak_google(opts: TtsOptions) -> anyhow::Result<()> {
    let key = std::env::var("GOOGLE_TTS_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| KeyringStore.get("google_tts_api_key").ok())
        .ok_or_else(|| anyhow::anyhow!("未配置 Google TTS API Key"))?;
    let lang_lc = opts.language.unwrap_or("zh-CN").to_ascii_lowercase();
    let lang = match lang_lc.as_str() {
        "zh" | "zh-cn" | "zh-hans" => "zh-CN",
        "zh-tw" | "zh-hant" => "zh-TW",
        "en" | "en-us" | "en-gb" => "en-US",
        "ja" => "ja-JP",
        "ko" => "ko-KR",
        _ => "en-US",
    };
    let body = serde_json::json!({
        "input": { "text": opts.text },
        "voice": { "languageCode": lang },
        "audioConfig": { "audioEncoding": "MP3" }
    });
    let url = format!("https://texttospeech.googleapis.com/v1/text:synthesize?key={key}");
    let v: serde_json::Value = crate::services::translate::ureq_agent()
        .post(url)
        .send_json(body)?
        .body_mut()
        .read_json()?;
    let b64 = v["audioContent"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Google TTS 无 audioContent: {v}"))?;
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)?;
    play_audio_bytes(&bytes, "mp3")
}

fn play_audio_bytes(bytes: &[u8], ext: &str) -> anyhow::Result<()> {
    let path = crate::app::paths::cache_dir().join(format!("tts-tmp.{ext}"));
    std::fs::write(&path, bytes)?;
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("afplay").arg(&path).output()?;
        if !out.status.success() {
            anyhow::bail!("afplay 失败");
        }
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        for (bin, args) in [
            ("ffplay", vec!["-nodisp", "-autoexit", "-loglevel", "quiet"]),
            ("mpg123", vec!["-q"]),
            ("paplay", vec![]),
            ("aplay", vec![]),
        ] {
            let mut cmd = Command::new(bin);
            cmd.args(&args).arg(&path);
            if cmd.output().map(|o| o.status.success()).unwrap_or(false) {
                return Ok(());
            }
        }
        anyhow::bail!("无法播放 TTS 音频（需要 ffplay / mpg123 / paplay）");
    }
    #[cfg(target_os = "windows")]
    {
        let p = path.display().to_string().replace('\'', "''");
        let ps = format!(
            r#"
$path = '{p}'
if ($path.ToLower().EndsWith('.wav')) {{
  (New-Object Media.SoundPlayer $path).PlaySync()
}} else {{
  Add-Type -AssemblyName PresentationCore
  $m = New-Object System.Windows.Media.MediaPlayer
  $m.Open([Uri]$path)
  $m.Play()
  $n = 0
  while (-not $m.NaturalDuration.HasTimeSpan -and $n -lt 50) {{ Start-Sleep -Milliseconds 50; $n++ }}
  if ($m.NaturalDuration.HasTimeSpan) {{
    Start-Sleep -Milliseconds ([int]$m.NaturalDuration.TimeSpan.TotalMilliseconds + 200)
  }} else {{
    Start-Sleep -Seconds 2
  }}
  $m.Close()
}}
"#
        );
        let out = Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", &ps])
            .output()?;
        if !out.status.success() {
            anyhow::bail!("Windows 播放 TTS 失败");
        }
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 探测 TTS 后端是否可用（不真正播放长文本）。
pub fn probe(backend: &str) -> anyhow::Result<()> {
    match backend.trim().to_ascii_lowercase().as_str() {
        "microsoft" | "azure" => {
            let _ = std::env::var("MICROSOFT_TTS_KEY")
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| KeyringStore.get("microsoft_tts_key").ok())
                .ok_or_else(|| anyhow::anyhow!("未配置 Microsoft TTS 密钥"))?;
            Ok(())
        }
        "google" | "google_cloud" => {
            let _ = std::env::var("GOOGLE_TTS_API_KEY")
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| KeyringStore.get("google_tts_api_key").ok())
                .ok_or_else(|| anyhow::anyhow!("未配置 Google TTS API Key"))?;
            Ok(())
        }
        _ => Ok(()),
    }
}
