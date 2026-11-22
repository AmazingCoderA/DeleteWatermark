//! Библиотечная часть движка: используется и CLI/GUI-бинарником, и Android-приложением.

pub mod config;
pub mod engine;
pub mod image_ops;
pub mod matcher;
pub mod web;

use jni::objects::{JClass, JString};
use jni::sys::jint;
use jni::JNIEnv;
use std::path::Path;

/// Запускает встроенный веб-сервер внутри Android-приложения.
/// WebView открывает http://127.0.0.1:5000 и общается с движком через него.
#[no_mangle]
pub extern "system" fn Java_com_amazingcoder_deletewatermark_MainActivity_startServer(
    mut env: JNIEnv,
    _class: JClass,
    samples_path: JString,
    port: jint,
) {
    let path: String = env
        .get_string(&samples_path)
        .map(|s| s.into())
        .unwrap_or_default();
    std::thread::spawn(move || {
        match engine::WatermarkRemover::new(Path::new(&path), config::EngineConfig::default()) {
            Ok(remover) => {
                if let Err(e) = web::run(remover, "127.0.0.1", port as u16) {
                    eprintln!("[Android] web server error: {}", e);
                }
            }
            Err(e) => eprintln!("[Android] samples error: {}", e),
        }
    });
}

/// Возвращает число загруженных образцов (для отладки).
#[no_mangle]
pub extern "system" fn Java_com_amazingcoder_deletewatermark_MainActivity_sampleCount(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    0
}
