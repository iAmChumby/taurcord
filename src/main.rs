#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{fs, path::PathBuf};

use base64::Engine;
use tauri::{
    Manager, Url, WebviewUrl, WebviewWindowBuilder,
    utils::config::Color,
    webview::NewWindowResponse,
};
use tauri_plugin_single_instance as single_instance;

const DISCORD_URL: &str = "https://discord.com/app";
const VENCORD_VERSION: &str = "1.15.2";

fn is_discord_url(url: &Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    matches!(
        url.host_str(),
        Some(host)
            if host == "discord.com"
                || host.ends_with(".discord.com")
                || host == "discord.gg"
                || host.ends_with(".discord.gg")
                || host == "discordapp.com"
                || host.ends_with(".discordapp.com")
    )
}

#[cfg(windows)]
fn open_system_browser(url: &str) {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .creation_flags(0x0800_0000)
        .spawn();
}

#[cfg(not(windows))]
fn open_system_browser(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

struct VencordAssets {
    js: String,
    css: String,
}

fn load_vencord_assets(app: &tauri::AppHandle) -> Result<VencordAssets, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(dir) = app.path().resource_dir() {
        candidates.push(dir.join("resources").join("vencord"));
        candidates.push(dir.join("vencord"));
    }
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        candidates.push(PathBuf::from(manifest_dir).join("resources").join("vencord"));
    }
    for dir in candidates {
        let js_path = dir.join("Vencord.js");
        let css_path = dir.join("Vencord.css");
        if js_path.is_file() && css_path.is_file() {
            let js = fs::read_to_string(&js_path)
                .map_err(|e| format!("failed to read {}: {e}", js_path.display()))?;
            let css = fs::read_to_string(&css_path)
                .map_err(|e| format!("failed to read {}: {e}", css_path.display()))?;
            eprintln!("[taurcord] Vencord assets loaded from {}", dir.display());
            return Ok(VencordAssets { js, css });
        }
    }
    Err("Vencord assets (Vencord.js / Vencord.css) not found in any resource location".into())
}

fn build_vencord_script(assets: &VencordAssets) -> String {
    assets.js.clone()
}

fn build_bridge_script(assets: &VencordAssets) -> String {
    let css_b64 = base64::engine::general_purpose::STANDARD.encode(assets.css.as_bytes());
    let mut script = String::with_capacity(css_b64.len() + 1024);
    script.push_str("(function(){");
    script.push_str("function __tcAddCss(){try{var s=document.createElement('style');s.id='taurcord-vencord-css';s.textContent=decodeURIComponent(escape(atob('");
    script.push_str(&css_b64);
    script.push_str("')));(document.head||document.documentElement).appendChild(s);}catch(e){try{console.error('[Taurcord] css inject failed:',e);}catch(_){}}}");
    script.push_str("if(document.head||document.documentElement){__tcAddCss();}else{document.addEventListener('DOMContentLoaded',__tcAddCss,{once:true});}");
    script.push_str(&format!(
        "try{{window.postMessage({{type:'vencord:meta',meta:{{EXTENSION_VERSION:'{VENCORD_VERSION}',EXTENSION_BASE_URL:'',RENDERER_CSS_URL:'data:text/css;base64,{css_b64}'}}}},'*');}}catch(e){{}}"
    ));
    if cfg!(debug_assertions) {
        script.push_str(
            "window.addEventListener('DOMContentLoaded',function(){setTimeout(function(){try{var b=document.createElement('div');b.style.cssText='position:fixed;bottom:8px;left:8px;z-index:999999;background:#000c;color:#fff;font:12px monospace;padding:4px 8px;border-radius:6px;pointer-events:none;';b.textContent=window.Vencord?'Vencord: OK':'Vencord: FAIL';document.body.appendChild(b);}catch(_){}},6000);});",
        );
    }
    script.push_str("})();");
    script
}

#[cfg(windows)]
fn attach_windows_hooks(win: &tauri::WebviewWindow) {
    let _ = win.with_webview(move |webview| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::*;
        use webview2_com::*;

        let controller = webview.controller();
        let core = match controller.CoreWebView2() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[taurcord] CoreWebView2 unavailable: {e}");
                return;
            }
        };

        let mut permission_token = 0i64;
        if let Err(e) = core.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(|_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                args.PermissionKind(&mut kind)?;
                if matches!(
                    kind,
                    COREWEBVIEW2_PERMISSION_KIND_MICROPHONE
                        | COREWEBVIEW2_PERMISSION_KIND_CAMERA
                        | COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS
                ) {
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
                }
                Ok(())
            })),
            &mut permission_token,
        ) {
            eprintln!("[taurcord] permission hook failed: {e}");
        }
    });
}

#[cfg(windows)]
fn apply_platform_builder<R: tauri::Runtime, M: tauri::Manager<R>>(
    builder: WebviewWindowBuilder<'_, R, M>,
) -> WebviewWindowBuilder<'_, R, M> {
    builder.additional_browser_args(
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required --disable-web-security",
    )
}

#[cfg(not(windows))]
fn apply_platform_builder<R: tauri::Runtime, M: tauri::Manager<R>>(
    builder: WebviewWindowBuilder<'_, R, M>,
) -> WebviewWindowBuilder<'_, R, M> {
    builder
}

fn create_main_window(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let assets = load_vencord_assets(app)?;
    let vencord_script = build_vencord_script(&assets);
    let bridge_script = build_bridge_script(&assets);
    let start_url: Url = match std::env::var("TAURCORD_URL") {
        Ok(url) if cfg!(debug_assertions) => url.parse().expect("valid TAURCORD_URL"),
        _ => DISCORD_URL.parse().expect("valid discord url"),
    };

    let debug_prefix = if cfg!(debug_assertions) { start_url.as_str().to_string() } else { String::new() };

    let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(start_url))
        .title("Taurcord")
        .inner_size(1280.0, 832.0)
        .min_inner_size(940.0, 500.0)
        .background_color(Color(0x31, 0x33, 0x38, 0xFF))
        .initialization_script(vencord_script)
        .initialization_script(bridge_script)
        .on_navigation(move |url| {
            if !debug_prefix.is_empty() && url.as_str().starts_with(debug_prefix.as_str()) {
                return true;
            }
            let allowed = is_discord_url(url);
            if !allowed {
                open_system_browser(url.as_str());
            }
            allowed
        })
        .on_new_window(|url, _features| {
            if !is_discord_url(&url) {
                open_system_browser(url.as_str());
            }
            NewWindowResponse::Deny
        });

    let win = apply_platform_builder(builder).build()?;
    #[cfg(windows)]
    attach_windows_hooks(&win);
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }))
        .setup(|app| Ok(create_main_window(app.handle())?))
        .run(tauri::generate_context!())
        .expect("error while running Taurcord");
}
