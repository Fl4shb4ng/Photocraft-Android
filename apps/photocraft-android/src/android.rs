//! Narrow Android ABI/JNI seam. Java provides system dialogs only, never editing UI.
use android_activity::AndroidApp;
use jni::{
    JNIEnv, JavaVM,
    objects::{JIntArray, JObject, JString, JValue},
};
use photocraft_doc::DocId;
use photocraft_engine::Session;
use photocraft_format::RecoveryStore;
use photocraft_ui_egui::{ExportSettings, Inbox, PhotocraftApp, Recovered, SaveResults, Services};
use std::{
    cell::RefCell,
    path::Path,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone)]
struct Bridge(AndroidApp);
impl Bridge {
    #[allow(unsafe_code)]
    fn with_env<T>(&self, f: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> jni::errors::Result<T>) -> Result<T, String> {
        // SAFETY: AndroidApp owns the VM and global Activity reference for its entire lifetime.
        // This wrapper keeps an AndroidApp clone alive throughout each call, attaches the
        // current thread, and never deletes the borrowed global reference or leaks locals.
        let vm = unsafe { JavaVM::from_raw(self.0.vm_as_ptr().cast()) }.map_err(|e| e.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let activity = unsafe { JObject::from_raw(self.0.activity_as_ptr().cast()) };
        let result = env.with_local_frame(32, |env| f(env, &activity));
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        result.map_err(|e| e.to_string())
    }
    fn open(&self) -> Result<(), String> {
        self.with_env(|env, activity| {
            env.call_method(activity, "requestOpen", "()V", &[])?;
            Ok(())
        })
    }
    fn save(&self, id: DocId, name: &str) -> Result<(), String> {
        self.with_env(|env, activity| {
            let id = env.new_string(id.0.to_string())?;
            let name = env.new_string(name)?;
            env.call_method(activity, "requestSave", "(Ljava/lang/String;Ljava/lang/String;)V", &[JValue::Object(&id), JValue::Object(&name)])?;
            Ok(())
        })
    }
    fn poll(&self) -> Result<String, String> {
        self.with_env(|env, activity| {
            let value = env.call_method(activity, "pollResult", "()Ljava/lang/String;", &[])?.l()?;
            let string = JString::from(value);
            let text = env.get_string(&string)?.into();
            Ok(text)
        })
    }
    fn insets(&self) -> Result<[i32; 4], String> {
        self.with_env(|env, activity| {
            let array = JIntArray::from(env.call_method(activity, "getSafeInsets", "()[I", &[])?.l()?);
            let mut values = [0; 4];
            env.get_int_array_region(&array, 0, &mut values)?;
            Ok(values)
        })
    }
    fn pending(&self) -> Result<bool, String> {
        self.with_env(|env, a| env.call_method(a, "hasPendingRequest", "()Z", &[])?.z())
    }
    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        let result: String = self.with_env(|env, a| {
            let path = env.new_string(path)?;
            let bytes = env.byte_array_from_slice(bytes)?;
            let result =
                env.call_method(a, "writeDocument", "(Ljava/lang/String;[B)Ljava/lang/String;", &[JValue::Object(&path), JValue::Object(&bytes)])?.l()?;
            Ok(env.get_string(&JString::from(result))?.into())
        })?;
        if result.is_empty() { Ok(()) } else { Err(result) }
    }
}

fn guard<T>(operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
        .map_err(|_| "The operation failed unexpectedly; your document remains open".to_string())?
}
fn services(bridge: Bridge, dir: &Path, inbox: Inbox, saves: SaveResults) -> Services {
    let prefs = dir.join("preferences.json");
    let load = prefs.clone();
    let recovery = Rc::new(RefCell::new(RecoveryStore::new(dir.join("Recovery"))));
    let (r1, r2, r3) = (recovery.clone(), recovery.clone(), recovery.clone());
    let (b1, b2, b3) = (bridge.clone(), bridge.clone(), bridge);
    Services {
        import: Some(Box::new(|name, bytes| guard(|| photocraft_io::import(name, bytes).map(|r| (r.document, r.warnings)).map_err(|e| e.to_string())))),
        export: Some(Box::new(|doc, path, settings: &ExportSettings| {
            guard(|| {
                let mut options = photocraft_io::ExportOptions::default();
                if let Some(q) = settings.jpeg_quality {
                    options.encode.jpeg_quality = q;
                }
                options.tiff_layers = settings.tiff_layers;
                options.xmp = if settings.xmp_all { photocraft_io::XmpEmbed::All } else { photocraft_io::XmpEmbed::None };
                photocraft_io::export(doc, path, &options).map(|r| (r.bytes, r.warnings)).map_err(|e| e.to_string())
            })
        })),
        pick_open_async: Some(Box::new(move || b1.open())),
        pick_save_async: Some(Box::new(move |id, name| b2.save(id, name))),
        save_results: Some(saves),
        inbox: Some(inbox),
        write: Some(Box::new(move |path, bytes| b3.write(path, bytes))),
        load_prefs: Some(Box::new(move || std::fs::read_to_string(&load).ok())),
        save_prefs: Some(Box::new(move |text| photocraft_format::atomic_write(&prefs, text.as_bytes()).map_err(|e| e.to_string()))),
        autosave: Some(Box::new(move |doc, revision, path| {
            r1.try_borrow_mut().map_err(|e| e.to_string())?.autosave(doc, revision, path.map(str::to_string));
            Ok(())
        })),
        discard_autosave: Some(Box::new(move |id| {
            if let Ok(mut store) = r2.try_borrow_mut() {
                let _ = store.discard(id);
            }
        })),
        recover: Some(Box::new(move || {
            r3.try_borrow_mut()
                .map(|s| s.recover())
                .unwrap_or_default()
                .into_iter()
                .map(|(e, doc)| Recovered { key: e.info.key, path: e.info.original_path, doc })
                .collect()
        })),
        adopt_autosave: Some(Box::new(move |id, key| {
            if let Ok(mut s) = recovery.try_borrow_mut() {
                s.adopt(id, key);
            }
        })),
        ..Default::default()
    }
}
struct AndroidEditor {
    editor: PhotocraftApp,
    bridge: Bridge,
    inbox: Inbox,
    saves: SaveResults,
    last_recovery: std::time::Instant,
}
impl AndroidEditor {
    fn poll(&mut self) -> Result<(), String> {
        // Bound a malicious/buggy provider's queue work per frame.
        for _ in 0..32 {
            let result = self.bridge.poll()?;
            if result.is_empty() {
                break;
            }
            let item: serde_json::Value = serde_json::from_str(&result).map_err(|e| e.to_string())?;
            let error = item["error"].as_str().unwrap_or("");
            match item["kind"].as_str().unwrap_or("") {
                "open" => {
                    let path = item["path"].as_str().ok_or("Missing cached file")?;
                    let bytes = photocraft_format::read_file(Path::new(path)).map_err(|e| e.to_string());
                    let _ = std::fs::remove_file(path);
                    let bytes = bytes?;
                    self.inbox.lock().unwrap_or_else(|e| e.into_inner()).push((item["name"].as_str().unwrap_or("image.png").into(), bytes));
                }
                "save" => {
                    let id = item["document"].as_str().and_then(|s| s.parse::<u64>().ok()).ok_or("Invalid document id")?;
                    let result =
                        if !error.is_empty() { Err(error.to_string()) } else { Ok(item["path"].as_str().filter(|s| !s.is_empty()).map(str::to_string)) };
                    self.saves.lock().unwrap_or_else(|e| e.into_inner()).push((DocId(id), result));
                }
                "error" => return Err(error.into()),
                _ => {}
            }
        }
        Ok(())
    }
}
impl eframe::App for AndroidEditor {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Err(e) = self.poll() {
            self.editor.ui.status = e;
            self.editor.ui.status_error = true;
        }
        // Android may kill a background process without an orderly desktop-style close.
        // Keep a recovery checkpoint while foregrounded, respecting the user's autosave setting.
        if self.editor.session.prefs().file_handling.autosave && self.last_recovery.elapsed() >= Duration::from_secs(10) {
            photocraft_ui_egui::prefs_ui::autosave_now(&mut self.editor);
            self.last_recovery = std::time::Instant::now();
        }
        self.editor.logic(ctx, frame);
        if self.editor.session.documents().iter().any(|d| d.is_dirty()) {
            ctx.request_repaint_after(Duration::from_secs(10));
        }
        if self.bridge.pending().unwrap_or(false) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if let Ok([left, top, right, bottom]) = self.bridge.insets() {
            let scale = ctx.pixels_per_point().max(0.1);
            input.safe_area_insets = Some(egui::SafeAreaInsets(egui::epaint::MarginF32 {
                left: left.max(0) as f32 / scale,
                top: top.max(0) as f32 / scale,
                right: right.max(0) as f32 / scale,
                bottom: bottom.max(0) as f32 / scale,
            }));
        }
        self.editor.raw_input_hook(ctx, input);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.editor.ui(ui, frame);
    }
}

// SAFETY: android-activity invokes this exported Rust ABI with its live AndroidApp on the
// native app thread. No pointers cross this entry point and no unsafe editing code is used.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_tag("PhotoCraft").with_max_level(log::LevelFilter::Info));
    let bridge = Bridge(app.clone());
    let Some(dir) = app.internal_data_path() else {
        log::error!("Android internal data directory unavailable");
        return;
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::error!("Cannot create settings directory: {e}");
        return;
    }
    let inbox = Arc::new(Mutex::new(Vec::new()));
    let saves = Arc::new(Mutex::new(Vec::new()));
    let platform = services(bridge.clone(), &dir, inbox.clone(), saves.clone());
    let options = eframe::NativeOptions { android_app: Some(app), renderer: eframe::Renderer::Wgpu, ..Default::default() };
    let result = eframe::run_native(
        "PhotoCraft",
        options,
        Box::new(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            let mut editor = PhotocraftApp::new(Session::new(), platform);
            editor.ui.mobile.enabled = true;
            editor.background_jobs = true;
            if let Some(state) = cc.wgpu_render_state.as_ref() {
                editor.set_wgpu(state.clone());
            }
            Ok(Box::new(AndroidEditor { editor, bridge, inbox, saves, last_recovery: std::time::Instant::now() }))
        }),
    );
    if let Err(e) = result {
        log::error!("PhotoCraft could not start: {e}");
    }
}
