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

fn js_b64(s: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(s.as_bytes());
    format!("decodeURIComponent(escape(atob('{b64}')))")
}

#[cfg(windows)]
fn load_vencord_migration() -> Option<String> {
    let base = std::path::PathBuf::from(std::env::var("APPDATA").ok()?).join("Vencord");
    let settings = fs::read_to_string(base.join("settings").join("settings.json")).ok()?;
    let quick_css = fs::read_to_string(base.join("settings").join("quickCss.css")).unwrap_or_default();
    let mut themes: Vec<(String, String)> = Vec::new();
    if let Ok(entries) = fs::read_dir(base.join("themes")) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_css = path.extension().map(|e| e == "css").unwrap_or(false);
            if !is_css {
                continue;
            }
            if let (Some(name), Ok(content)) = (
                path.file_name().map(|n| n.to_string_lossy().to_string()),
                fs::read_to_string(&path),
            ) {
                themes.push((name, content));
            }
        }
    }
    eprintln!(
        "[taurcord] migrating desktop Vencord data: settings {}B, quickCss {}B, {} theme(s)",
        settings.len(),
        quick_css.len(),
        themes.len()
    );

    let mut script = String::with_capacity(4096 + settings.len() + quick_css.len());
    script.push_str("(function(){");
    script.push_str("if(localStorage.getItem('__taurcordVencordMigrated'))return;");
    script.push_str("function idb(db,store,cb){var r=indexedDB.open(db);r.onupgradeneeded=function(){r.result.createObjectStore(store);};r.onsuccess=function(){var t=r.result.transaction(store,'readwrite');cb(t.objectStore(store));t.oncomplete=function(){r.result.close();};};}");
    script.push_str("try{");
    script.push_str(&format!(
        "localStorage.setItem('VencordSettings',{});",
        js_b64(&settings)
    ));
    if !quick_css.trim().is_empty() {
        script.push_str(&format!(
            "idb('VencordData','VencordStore',function(s){{s.put({},'VencordQuickCss');}});",
            js_b64(&quick_css)
        ));
    }
    if !themes.is_empty() {
        script.push_str("idb('VencordThemes','VencordThemeData',function(s){");
        for (name, content) in &themes {
            let name_js = format!("'{}'", name.replace('\\', "\\\\").replace('\'', "\\'"));
            script.push_str(&format!("s.put({},{});", js_b64(content), name_js));
        }
        script.push_str("});");
    }
    script.push_str("localStorage.setItem('__taurcordVencordMigrated',String(Date.now()));");
    script.push_str("console.info('[Taurcord] Vencord data migrated from desktop Vencord');");
    script.push_str("}catch(e){console.error('[Taurcord] Vencord migration failed:',e);}");
    script.push_str("})();");
    Some(script)
}

#[cfg(not(windows))]
fn load_vencord_migration() -> Option<String> {
    None
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
    script.push_str("function tcTitlebar(){if(document.getElementById('taurcord-titlebar'))return;var st=document.createElement('style');st.id='taurcord-titlebar-css';st.textContent='#app-mount{top:36px!important}#taurcord-titlebar{position:fixed;top:0;left:0;right:0;height:36px;z-index:6000;user-select:none;-webkit-user-select:none}#taurcord-titlebar .tcb{position:absolute;top:0;width:46px;height:36px;display:flex;align-items:center;justify-content:center;color:var(--interactive-normal,#b5bac1)}#taurcord-titlebar .tcb svg{width:10px;height:10px}#taurcord-titlebar .tcb-min{right:92px}#taurcord-titlebar .tcb-max{right:46px}#taurcord-titlebar .tcb-close{right:0}';(document.head||document.documentElement).appendChild(st);var bar=document.createElement('div');bar.id='taurcord-titlebar';bar.innerHTML='<div class=\"tcb tcb-min\"><svg viewBox=\"0 0 10 10\"><path d=\"M0 5h10\" stroke=\"currentColor\"/></svg></div><div class=\"tcb tcb-max\"><svg viewBox=\"0 0 10 10\"><rect x=\"0.5\" y=\"0.5\" width=\"9\" height=\"9\" fill=\"none\" stroke=\"currentColor\"/></svg></div><div class=\"tcb tcb-close\"><svg viewBox=\"0 0 10 10\"><path d=\"M0 0l10 10M10 0L0 10\" stroke=\"currentColor\"/></svg></div>';(document.body||document.documentElement).appendChild(bar);document.addEventListener('fullscreenchange',function(){bar.style.display=document.fullscreenElement?'none':'block';});}");
    script.push_str("if(document.body){tcTitlebar();}else{document.addEventListener('DOMContentLoaded',tcTitlebar,{once:true});}");
    script.push_str(&format!(
        "try{{window.postMessage({{type:'vencord:meta',meta:{{EXTENSION_VERSION:'{VENCORD_VERSION}',EXTENSION_BASE_URL:'',RENDERER_CSS_URL:'data:text/css;base64,{css_b64}'}}}},'*');}}catch(e){{}}"
    ));
    if cfg!(debug_assertions) {
        script.push_str(
            "window.addEventListener('DOMContentLoaded',function(){setTimeout(function(){try{var b=document.createElement('div');b.style.cssText='position:fixed;bottom:8px;left:8px;z-index:999999;background:#000c;color:#fff;font:12px monospace;padding:4px 8px;border-radius:6px;pointer-events:none;';document.body.appendChild(b);var parts=['Vencord: '+(window.Vencord?'OK':'FAIL')];function render(){b.textContent=parts.join(' | ');}var v=window.Vencord;if(v){try{var n=0,tot=0,ps=v.Plugins&&v.Plugins.plugins;if(ps)for(var k in ps){tot++;if(ps[k].started)n++;}parts.push('plugins '+n+'/'+tot);}catch(e){}try{if(window.VencordNative&&VencordNative.themes)VencordNative.themes.getThemesList().then(function(l){parts.push('themes '+l.length);render();}).catch(function(){});}catch(e){}try{if(window.VencordNative&&VencordNative.quickCss)VencordNative.quickCss.get().then(function(c){if(c&&c.length)parts.push('qcss '+c.length+'B');render();}).catch(function(){});}catch(e){}}render();}catch(_){}},6000);});",
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
        use windows::Win32::Foundation::HWND;

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

        let mut host = HWND::default();
        if controller.ParentWindow(&mut host).is_ok() {
            install_titlebar_hooks(host);
        }
    });
}

#[cfg(windows)]
const TITLEBAR_SUBCLASS_ID: usize = 0x5443_4342;

#[cfg(windows)]
unsafe fn install_titlebar_hooks(host: windows::Win32::Foundation::HWND) {
    use windows::core::BOOL;
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::Shell::SetWindowSubclass;
    use windows::Win32::UI::WindowsAndMessaging::{EnumChildWindows, SetTimer};

    unsafe extern "system" fn enum_cb(
        hwnd: windows::Win32::Foundation::HWND,
        _: LPARAM,
    ) -> BOOL {
        subclass_if_relevant(hwnd);
        BOOL(1)
    }

    subclass_if_relevant(host);
    let _ = EnumChildWindows(Some(host), Some(enum_cb), LPARAM(0));
    let _ = SetTimer(Some(host), 1, 5000, None);
}

#[cfg(windows)]
unsafe fn subclass_if_relevant(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::Shell::SetWindowSubclass;
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;

    let mut buf = [0u16; 64];
    let n = GetClassNameW(hwnd, &mut buf);
    let cls = String::from_utf16_lossy(&buf[..n as usize]);
    if cls.starts_with("Chrome_WidgetWin")
        || cls == "Chrome_RenderWidgetHostHWND"
        || cls == "WRY_WEBVIEW"
        || cls == "Intermediate D3D Window"
    {
        let _ = SetWindowSubclass(hwnd, Some(titlebar_proc), TITLEBAR_SUBCLASS_ID, 0);
    }
}

#[cfg(windows)]
unsafe extern "system" fn titlebar_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
    _: usize,
    _: usize,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::{LRESULT, RECT, WPARAM};
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::Shell::DefSubclassProc;
    use windows::Win32::UI::WindowsAndMessaging::*;

    unsafe {
        match msg {
            WM_NCHITTEST => {
                let res = DefSubclassProc(hwnd, msg, wparam, lparam);
                if res.0 as u32 == HTCLIENT || res.0 == 0 {
                    let root = GetAncestor(hwnd, GA_ROOT);
                    let mut rect = RECT::default();
                    if GetWindowRect(root, &mut rect).is_ok() {
                        let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
                        let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
                        let rel_y = y - rect.top;
                        let rel_x = x - rect.left;
                        let dpi = GetDpiForWindow(root);
                        let bar_h = 36i32 * dpi as i32 / 96;
                        if rel_y >= 0 && rel_y < bar_h {
                            let w = rect.right - rect.left;
                            let bw = 46i32 * dpi as i32 / 96;
                            if rel_x >= w - bw {
                                return LRESULT(HTCLOSE as _);
                            }
                            if rel_x >= w - 2 * bw {
                                return LRESULT(HTMAXBUTTON as _);
                            }
                            if rel_x >= w - 3 * bw {
                                return LRESULT(HTMINBUTTON as _);
                            }
                            return LRESULT(HTCAPTION as _);
                        }
                    }
                }
                res
            }
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK | WM_NCRBUTTONDOWN => {
                let hit = wparam.0 as u32;
                let root = GetAncestor(hwnd, GA_ROOT);
                if msg == WM_NCLBUTTONDOWN {
                    let sc = match hit {
                        HTMINBUTTON => Some(SC_MINIMIZE),
                        HTMAXBUTTON => {
                            if IsZoomed(root).into() {
                                Some(SC_RESTORE)
                            } else {
                                Some(SC_MAXIMIZE)
                            }
                        }
                        HTCLOSE => Some(SC_CLOSE),
                        _ => None,
                    };
                    if let Some(sc) = sc {
                        let _ = PostMessageW(Some(root), WM_SYSCOMMAND, WPARAM(sc as _), lparam);
                        return LRESULT(0);
                    }
                }
                if hit == HTCAPTION {
                    let _ = PostMessageW(Some(root), msg, wparam, lparam);
                    return LRESULT(0);
                }
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
            WM_TIMER => {
                let root = GetAncestor(hwnd, GA_ROOT);
                install_titlebar_hooks(root);
                LRESULT(0)
            }
            _ => DefSubclassProc(hwnd, msg, wparam, lparam),
        }
    }
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

    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(start_url))
        .title("Taurcord")
        .inner_size(1280.0, 832.0)
        .min_inner_size(940.0, 500.0)
        .decorations(false)
        .background_color(Color(0x31, 0x33, 0x38, 0xFF));
    if let Some(migration) = load_vencord_migration() {
        builder = builder.initialization_script(migration);
    }
    let builder = builder
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
