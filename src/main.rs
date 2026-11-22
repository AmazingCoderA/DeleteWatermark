//! Умный удалитель водяных знаков v8.0 — Rust-порт DeleteGdzSuper6.py.
//!
//! Режимы: cli (пакетная обработка), web (HTTP-сервер для локальной сети),
//! gui (десктопное окно; кроме Android).

#[cfg(all(not(target_os = "android"), feature = "gui"))]
mod gui;

use anyhow::{anyhow, Result};
use clap::Parser;
use delete_watermark_android::{config, engine, web};
use config::{is_supported, resolve_samples_dir, EngineConfig};
use engine::WatermarkRemover;
use std::fs;
use std::path::PathBuf;

#[cfg(all(not(target_os = "android"), feature = "gui"))]
const DEFAULT_MODE: &str = "gui";
#[cfg(not(all(not(target_os = "android"), feature = "gui")))]
const DEFAULT_MODE: &str = "cli";

#[derive(Parser, Debug)]
#[command(
    name = "delete-watermark",
    version = "8.0.0",
    about = "Удаление водяных знаков по базе образцов (CLI / Web / GUI)"
)]
struct Args {
    /// Режим: cli, web или gui
    #[arg(long, default_value = DEFAULT_MODE)]
    mode: String,

    /// Путь к базе образцов (папка с PNG/JPG-шаблонами)
    #[arg(long, value_name = "ПАПКА")]
    samples: Option<PathBuf>,

    /// Файлы или папки для обработки (только режим cli)
    #[arg(long, value_name = "ПУТЬ")]
    input: Vec<PathBuf>,

    /// Папка для результатов (иначе файл *_cleaned рядом с оригиналом)
    #[arg(long, value_name = "ПАПКА")]
    output: Option<PathBuf>,

    /// Порог совпадения шаблона, 0..1 (по умолчанию 0.75)
    #[arg(long)]
    threshold: Option<f32>,

    /// Порог перекрытия NMS (по умолчанию 0.3)
    #[arg(long)]
    nms: Option<f32>,

    /// Минимальный масштаб шаблона (по умолчанию 0.6)
    #[arg(long)]
    scale_min: Option<f32>,

    /// Максимальный масштаб шаблона (по умолчанию 1.4)
    #[arg(long)]
    scale_max: Option<f32>,

    /// Число шагов масштаба (по умолчанию 25)
    #[arg(long)]
    scale_steps: Option<usize>,

    /// Радиус inpaint (по умолчанию 7)
    #[arg(long)]
    inpaint_radius: Option<i32>,

    /// Пирамида поиска: 1 = точно в полном разрешении, 2 = быстро (по умолчанию), 3 = очень быстро
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=4))]
    pyramid: Option<u32>,

    /// Максимальная аккуратность: точный поиск, порог 0.82, маска только по знаку
    #[arg(long)]
    precise: bool,

    /// Только вывести список загруженных образцов и выйти
    #[arg(long)]
    list_samples: bool,

    /// Адрес привязки веб-сервера (по умолчанию 0.0.0.0)
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// Порт веб-сервера (по умолчанию 5000)
    #[arg(long, default_value_t = 5000)]
    port: u16,
}

fn engine_config(args: &Args) -> EngineConfig {
    let mut c = EngineConfig::default();
    if let Some(v) = args.threshold {
        c.match_threshold = v;
    }
    if let Some(v) = args.nms {
        c.overlap_threshold = v;
    }
    if let Some(v) = args.scale_min {
        c.scale_min = v;
    }
    if let Some(v) = args.scale_max {
        c.scale_max = v;
    }
    if let Some(v) = args.scale_steps {
        c.scale_steps = v;
    }
    if let Some(v) = args.inpaint_radius {
        c.inpaint_radius = v;
    }
    if let Some(v) = args.pyramid {
        c.pyramid = v;
    }
    if args.precise {
        c.pyramid = 1;
        c.match_threshold = c.match_threshold.max(0.82);
        c.mask_pad = 0;
    }
    c
}

fn collect_inputs(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for p in paths {
        if p.is_file() {
            if is_supported(p) {
                files.push(p.clone());
            }
        } else if p.is_dir() {
            let mut entries: Vec<_> = fs::read_dir(p)?.filter_map(|e| e.ok()).collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let path = e.path();
                if path.is_file() && is_supported(&path) {
                    files.push(path);
                }
            }
        } else {
            return Err(anyhow!("Путь не существует: {}", p.display()));
        }
    }
    Ok(files)
}

fn run_cli(args: &Args, remover: &WatermarkRemover) -> Result<()> {
    let files = collect_inputs(&args.input)?;
    if files.is_empty() {
        return Err(anyhow!(
            "Не указаны входные файлы. Используйте --input <файл|папка> [--input ...]"
        ));
    }
    let total = files.len();
    let mut ok = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for (i, file) in files.iter().enumerate() {
        let name = file.file_name().and_then(|s| s.to_str()).unwrap_or("?");
        println!("[{} {}/{}] {}", "▸", i + 1, total, name);
        let output = args.output.as_ref().map(|dir| {
            let fname = file
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("image.png");
            dir.join(fname)
        });
        match remover.process_file(file, output.as_deref()) {
            Ok((msg, out, _)) => {
                ok += 1;
                println!("   ✓ {} → {}", msg, out.display());
            }
            Err(e) => {
                errors.push(format!("{}: {}", name, e));
                println!("   ✗ {}", e);
            }
        }
    }

    println!("\nОбработано успешно: {} из {}", ok, total);
    if !errors.is_empty() {
        println!("\nОшибки ({}):", errors.len());
        for e in errors.iter().take(5) {
            println!("  • {}", e);
        }
        if errors.len() > 5 {
            println!("  ... и ещё {}", errors.len() - 5);
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    println!("============================================================");
    println!("УДАЛИТЕЛЬ ВОДЯНЫХ ЗНАКОВ v8.0 (Rust)");
    println!("============================================================");

    let samples_dir = resolve_samples_dir(args.samples.as_deref())?;
    let config = engine_config(&args);
    let remover = WatermarkRemover::new(&samples_dir, config)?;
    println!("[INFO] {}", remover.info());
    for t in &remover.templates {
        println!("  ✓ {}", t.name);
    }

    if args.list_samples {
        return Ok(());
    }

    match args.mode.as_str() {
        "cli" => run_cli(&args, &remover)?,
        "web" => web::run(remover, &args.host, args.port)?,
        "gui" => {
            #[cfg(all(not(target_os = "android"), feature = "gui"))]
            {
                gui::run(remover, args.samples.as_deref())?;
            }
            #[cfg(not(all(not(target_os = "android"), feature = "gui")))]
            {
                return Err(anyhow!(
                    "GUI недоступен (сборка без графики или Android). Используйте --mode web или --mode cli"
                ));
            }
        }
        other => return Err(anyhow!("Неизвестный режим: {} (cli|web|gui)", other)),
    }
    Ok(())
}
