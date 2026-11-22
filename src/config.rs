//! Константы и конфигурация движка.
//! Значения по умолчанию повторяют `DeleteGdzSuper6.py`.

use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};

/// Порог совпадения шаблона (MATCH_THRESHOLD в Python).
pub const MATCH_THRESHOLD: f32 = 0.75;
/// Порог перекрытия для NMS (OVERLAP_THRESHOLD_NMS в Python).
pub const OVERLAP_THRESHOLD_NMS: f32 = 0.3;
/// Поддерживаемые форматы (SUPPORTED_FORMATS в Python).
pub const SUPPORTED_FORMATS: [&str; 5] = [".png", ".jpg", ".jpeg", ".bmp", ".tiff"];

pub const SCALE_MIN: f32 = 0.6;
pub const SCALE_MAX: f32 = 1.4;
pub const SCALE_STEPS: usize = 25;
pub const INPAINT_RADIUS: i32 = 7;
pub const MASK_PAD: i32 = 2;

/// Все настройки поиска/удаления. CLI и GUI могут их переопределять.
#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub match_threshold: f32,
    pub overlap_threshold: f32,
    pub scale_min: f32,
    pub scale_max: f32,
    pub scale_steps: usize,
    pub inpaint_radius: i32,
    pub mask_pad: i32,
    /// Пирамида: 1 = точный поиск в полный рост, 2 = поиск на половинном
    /// разрешении с уточнением кандидатов (по умолчанию 2, в ~4 раза быстрее).
    pub pyramid: u32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            match_threshold: MATCH_THRESHOLD,
            overlap_threshold: OVERLAP_THRESHOLD_NMS,
            scale_min: SCALE_MIN,
            scale_max: SCALE_MAX,
            scale_steps: SCALE_STEPS,
            inpaint_radius: INPAINT_RADIUS,
            mask_pad: MASK_PAD,
            pyramid: 2,
        }
    }
}

/// Имя папки с образцами по умолчанию (как в Python-скрипте).
pub const SAMPLES_DIR_NAME: &str = "GDZ_SAMPLES";

/// Где искать базу образцов:
/// 1) явный путь (флаг --samples);
/// 2) переменная окружения WATERMARK_SAMPLES;
/// 3) <каталог исполняемого файла>/assets/GDZ_SAMPLES;
/// 4) <текущий каталог>/assets/GDZ_SAMPLES;
/// 5) <каталог исполняемого файла>/GDZ_SAMPLES;
/// 6) <текущий каталог>/GDZ_SAMPLES.
pub fn resolve_samples_dir(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }
    if let Ok(p) = env::var("WATERMARK_SAMPLES") {
        if !p.trim().is_empty() {
            return Ok(PathBuf::from(p));
        }
    }

    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let cwd = env::current_dir().ok();

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = &exe_dir {
        candidates.push(dir.join("assets").join(SAMPLES_DIR_NAME));
        candidates.push(dir.join(SAMPLES_DIR_NAME));
    }
    if let Some(dir) = &cwd {
        candidates.push(dir.join("assets").join(SAMPLES_DIR_NAME));
        candidates.push(dir.join(SAMPLES_DIR_NAME));
    }

    for c in &candidates {
        if c.is_dir() {
            return Ok(c.clone());
        }
    }

    // Если ничего не найдено — вернём самый разумный путь по умолчанию,
    // чтобы сообщение об ошибке было понятным.
    candidates
        .into_iter()
        .next()
        .context("Не удалось определить каталог с образцами")
}

/// Расширение файла в нижнем регистре.
pub fn extension_lower(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{}", s.to_lowercase()))
        .unwrap_or_default()
}

pub fn is_supported(path: &Path) -> bool {
    SUPPORTED_FORMATS.contains(&extension_lower(path).as_str())
}
