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

struct VencordMigration {
    settings: String,
    quick_css: String,
    themes: Vec<(String, String)>,
}

#[cfg(windows)]
fn load_vencord_migration() -> Option<VencordMigration> {
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
    Some(VencordMigration {
        settings,
        quick_css,
        themes,
    })
}

#[cfg(not(windows))]
fn load_vencord_migration() -> Option<VencordMigration> {
    None
}

fn build_bootstrap_scripts(assets: &VencordAssets, migration: Option<&VencordMigration>) -> (String, String) {
    let css_b64 = base64::engine::general_purpose::STANDARD.encode(assets.css.as_bytes());

    let mut vencord_fn = String::with_capacity(assets.js.len() + 64);
    vencord_fn.push_str("function __tcVencord(){");
    vencord_fn.push_str(&assets.js);
    vencord_fn.push_str(";window.Vencord=Vencord;}");

    let mut post_js = String::with_capacity(2048);
    post_js.push_str(&format!(
        "try{{window.postMessage({{type:'vencord:meta',meta:{{EXTENSION_VERSION:'{VENCORD_VERSION}',EXTENSION_BASE_URL:'',RENDERER_CSS_URL:'data:text/css;base64,{css_b64}'}}}},'*');}}catch(e){{}}"
    ));
    if cfg!(debug_assertions) {
        post_js.push_str(
            "window.addEventListener('DOMContentLoaded',function(){setTimeout(function(){try{var b=document.createElement('div');b.style.cssText='position:fixed;bottom:8px;left:8px;z-index:999999;background:#000c;color:#fff;font:12px monospace;padding:4px 8px;border-radius:6px;pointer-events:none;';document.body.appendChild(b);var parts=['Vencord: '+(window.Vencord?'OK':'FAIL')];parts.push('ipc: '+(window.__TAURI_INTERNALS__?'yes':'NO'));function render(){b.textContent=parts.join(' | ');}var v=window.Vencord;if(v){try{var n=0,tot=0,ps=v.Plugins&&v.Plugins.plugins;if(ps)for(var k in ps){tot++;if(ps[k].started)n++;}parts.push('plugins '+n+'/'+tot);}catch(e){}try{if(window.VencordNative&&VencordNative.themes)VencordNative.themes.getThemesList().then(function(l){parts.push('themes '+l.length);render();}).catch(function(){});}catch(e){}}}render();}catch(_){}},6000);});",
        );
    }
    let post_b64 = base64::engine::general_purpose::STANDARD.encode(post_js.as_bytes());

    let mut gate = String::with_capacity(8192);
    gate.push_str("(function(){");
    gate.push_str("function tcPost(){var s=document.createElement('script');s.textContent=decodeURIComponent(escape(atob('");
    gate.push_str(&post_b64);
    gate.push_str("')));(document.head||document.documentElement).appendChild(s);}");
    gate.push_str("function tcGo(){if(window.__TC_DONE__)return;window.__TC_DONE__=true;try{__tcVencord();}catch(e){try{console.error('[Taurcord] Vencord failed:',e);}catch(_){}}tcPost();}");

    gate.push_str("function tcAddCss(){try{var s=document.createElement('style');s.id='taurcord-vencord-css';s.textContent=decodeURIComponent(escape(atob('");
    gate.push_str(&css_b64);
    gate.push_str("')));(document.head||document.documentElement).appendChild(s);}catch(e){try{console.error('[Taurcord] css inject failed:',e);}catch(_){}}}");
    gate.push_str("if(document.head||document.documentElement){tcAddCss();}else{document.addEventListener('DOMContentLoaded',tcAddCss,{once:true});}");

    gate.push_str("function tcTitlebar(){if(document.getElementById('taurcord-titlebar'))return;var st=document.createElement('style');st.id='taurcord-titlebar-css';st.textContent='#app-mount{top:36px!important;height:calc(100% - 36px)!important}#taurcord-titlebar{position:fixed;top:0;left:0;right:0;height:36px;z-index:6000;user-select:none;-webkit-user-select:none}#taurcord-titlebar .tcb{position:absolute;top:0;width:46px;height:36px;display:flex;align-items:center;justify-content:center;color:var(--interactive-normal,#b5bac1);cursor:default}#taurcord-titlebar .tcb:hover{color:var(--interactive-hover,#dbdee1)}#taurcord-titlebar .tcb-close:hover{color:#fff;background:var(--status-danger,#da373c)}#taurcord-titlebar .tcb svg{width:10px;height:10px}#taurcord-titlebar .tcb-min{right:92px}#taurcord-titlebar .tcb-max{right:46px}#taurcord-titlebar .tcb-close{right:0}';(document.head||document.documentElement).appendChild(st);var bar=document.createElement('div');bar.id='taurcord-titlebar';bar.innerHTML='<div class=\"tcb tcb-min\"><svg viewBox=\"0 0 10 10\"><path d=\"M0 5h10\" stroke=\"currentColor\"/></svg></div><div class=\"tcb tcb-max\"><svg class=\"tc-max-g\" viewBox=\"0 0 10 10\"><rect x=\"0.5\" y=\"0.5\" width=\"9\" height=\"9\" fill=\"none\" stroke=\"currentColor\"/></svg><svg class=\"tc-res-g\" viewBox=\"0 0 10 10\" style=\"display:none\"><path d=\"M2.5 2.5h5v5\" fill=\"none\" stroke=\"currentColor\"/><rect x=\"0.5\" y=\"4.5\" width=\"5\" height=\"5\" fill=\"#00000000\" stroke=\"currentColor\"/></svg></div><div class=\"tcb tcb-close\"><svg viewBox=\"0 0 10 10\"><path d=\"M0 0l10 10M10 0L0 10\" stroke=\"currentColor\"/></svg></div>';(document.body||document.documentElement).appendChild(bar);var inv=function(c){try{window.__TAURI_INTERNALS__.invoke(c)}catch(e){}};var mg=bar.querySelector('.tc-max-g'),rg=bar.querySelector('.tc-res-g');function syncMax(){var m=window.innerWidth>=screen.availWidth-24&&window.innerHeight>=screen.availHeight-24;mg.style.display=m?'none':'block';rg.style.display=m?'block':'none';}bar.querySelector('.tcb-min').addEventListener('click',function(){inv('plugin:window|minimize')});bar.querySelector('.tcb-max').addEventListener('click',function(){inv('plugin:window|toggle_maximize');setTimeout(syncMax,150)});bar.querySelector('.tcb-close').addEventListener('click',function(){inv('plugin:window|close')});bar.addEventListener('mousedown',function(e){if(e.button===0&&e.target===bar){inv('plugin:window|start_dragging')}});bar.addEventListener('dblclick',function(e){if(e.target===bar){inv('plugin:window|toggle_maximize');setTimeout(syncMax,150)}});window.addEventListener('resize',syncMax);syncMax();}");
    gate.push_str("if(document.body){tcTitlebar();}else{document.addEventListener('DOMContentLoaded',tcTitlebar,{once:true});}");
    gate.push_str("try{if(!sessionStorage.getItem('__tcCspRetry')){fetch('data:text/css;base64,LnR7fQ==').catch(function(){sessionStorage.setItem('__tcCspRetry','1');location.reload();});}}catch(e){}");

    match migration {
        Some(m) => {
            eprintln!(
                "[taurcord] importing desktop Vencord data: settings {}B, quickCss {}B, {} theme(s)",
                m.settings.len(),
                m.quick_css.len(),
                m.themes.len()
            );
            gate.push_str("var ops=0;");
            gate.push_str("function tcDone(){ops--;if(ops<=0)tcGo();}");
            gate.push_str("function tcIdb(db,store,cb){ops++;var r=indexedDB.open(db);r.onupgradeneeded=function(){r.result.createObjectStore(store);};r.onsuccess=function(){try{var t=r.result.transaction(store,'readwrite');cb(t.objectStore(store));t.oncomplete=function(){r.result.close();tcDone();};}catch(e){tcDone();}};r.onerror=function(){tcDone();};}");
            gate.push_str(&format!(
                "try{{localStorage.setItem('VencordSettings',{});}}catch(e){{}}",
                js_b64(&m.settings)
            ));
            if !m.quick_css.trim().is_empty() {
                gate.push_str(&format!(
                    "tcIdb('VencordData','VencordStore',function(s){{s.put({},'VencordQuickCss');}});",
                    js_b64(&m.quick_css)
                ));
            }
            if !m.themes.is_empty() {
                gate.push_str("tcIdb('VencordThemes','VencordThemeData',function(s){");
                for (name, content) in &m.themes {
                    let name_js = format!("'{}'", name.replace('\\', "\\\\").replace('\'', "\\'"));
                    gate.push_str(&format!("s.put({},{});", js_b64(content), name_js));
                }
                gate.push_str("});");
            }
            gate.push_str("if(ops<=0)tcGo();setTimeout(tcGo,1500);");
        }
        None => {
            gate.push_str("tcGo();");
        }
    }

    gate.push_str("})();");
    (vencord_fn, gate)
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

        use windows_core::{HSTRING, PCWSTR};
        let cdp_result = core.CallDevToolsProtocolMethod(
            PCWSTR(HSTRING::from("Page.setBypassCSP").as_ptr()),
            PCWSTR(HSTRING::from("{\"enabled\":true}").as_ptr()),
            &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(()))),
        );
        if let Err(e) = cdp_result {
            eprintln!("[taurcord] setBypassCSP failed: {e}");
        }
    });
}

#[cfg(windows)]
fn apply_platform_builder<R: tauri::Runtime, M: tauri::Manager<R>>(
    builder: WebviewWindowBuilder<'_, R, M>,
) -> WebviewWindowBuilder<'_, R, M> {
    let mut args = String::from(
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required --disable-web-security",
    );
    if cfg!(debug_assertions) {
        args.push_str(" --remote-debugging-port=9223");
    }
    builder.additional_browser_args(&args)
}

#[cfg(not(windows))]
fn apply_platform_builder<R: tauri::Runtime, M: tauri::Manager<R>>(
    builder: WebviewWindowBuilder<'_, R, M>,
) -> WebviewWindowBuilder<'_, R, M> {
    builder
}

fn create_main_window(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let assets = load_vencord_assets(app)?;
    let migration = load_vencord_migration();
    let (vencord_fn, gate) = build_bootstrap_scripts(&assets, migration.as_ref());
    let start_url: Url = match std::env::var("TAURCORD_URL") {
        Ok(url) if cfg!(debug_assertions) => url.parse().expect("valid TAURCORD_URL"),
        _ => DISCORD_URL.parse().expect("valid discord url"),
    };

    let debug_prefix = if cfg!(debug_assertions) { start_url.as_str().to_string() } else { String::new() };

    let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(start_url))
        .title("Taurcord")
        .inner_size(1280.0, 832.0)
        .min_inner_size(940.0, 500.0)
        .decorations(false)
        .background_color(Color(0x31, 0x33, 0x38, 0xFF))
        .initialization_script(vencord_fn)
        .initialization_script(gate)
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
