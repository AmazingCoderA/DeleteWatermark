//! Десктопный GUI (egui/eframe) — аналог CustomTkinter-интерфейса из Python.

use delete_watermark_android::config::{is_supported, EngineConfig};
use delete_watermark_android::engine::WatermarkRemover;
use anyhow::Result;
use eframe::egui;
use rfd::{FileDialog, MessageDialog};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

enum Msg {
    Progress(f32, String),
    Done {
        ok: usize,
        total: usize,
        errors: Vec<String>,
    },
    Cancelled,
}

struct App {
    remover: Arc<WatermarkRemover>,
    config: EngineConfig,
    samples_dir: PathBuf,
    status: String,
    progress: f32,
    busy: bool,
    rx: Option<mpsc::Receiver<Msg>>,
    cancel: Arc<AtomicBool>,
}

impl App {
    fn new(remover: WatermarkRemover, samples_dir: PathBuf) -> Self {
        let config = remover.config.clone();
        let count = remover.templates.len();
        Self {
            remover: Arc::new(remover),
            config,
            samples_dir,
            status: format!("Готов к работе. Образцов: {}", count),
            progress: 0.0,
            busy: false,
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    fn choose_samples(&mut self) {
        if let Some(dir) = FileDialog::new().pick_folder() {
            match WatermarkRemover::new(&dir, self.config.clone()) {
                Ok(r) => {
                    self.status = format!("База образцов: {} ({} шт.)", dir.display(), r.templates.len());
                    self.samples_dir = dir;
                    self.remover = Arc::new(r);
                }
                Err(e) => {
                    self.status = format!("Ошибка загрузки базы: {}", e);
                }
            }
        }
    }

    fn start(&mut self, mode: &str) {
        if self.busy {
            return;
        }
        let files: Vec<PathBuf> = if mode == "files" {
            FileDialog::new()
                .add_filter("Изображения", &["png", "jpg", "jpeg", "bmp", "tiff"])
                .pick_files()
                .unwrap_or_default()
        } else {
            match FileDialog::new().pick_folder() {
                Some(dir) => {
                    let mut v: Vec<_> = fs::read_dir(&dir)
                        .into_iter()
                        .flatten()
                        .filter_map(|e| e.ok())
                        .map(|e| e.path())
                        .filter(|p| p.is_file() && is_supported(p))
                        .collect();
                    v.sort();
                    v
                }
                None => Vec::new(),
            }
        };
        if files.is_empty() {
            return;
        }

        let (tx, rx) = mpsc::channel();
        let remover = Arc::clone(&self.remover);
        let cancel = Arc::clone(&self.cancel);
        cancel.store(false, Ordering::SeqCst);
        let total = files.len();

        thread::spawn(move || {
            let mut ok = 0usize;
            let mut errors = Vec::new();
            for (i, file) in files.iter().enumerate() {
                if cancel.load(Ordering::SeqCst) {
                    let _ = tx.send(Msg::Cancelled);
                    return;
                }
                let name = file.file_name().and_then(|s| s.to_str()).unwrap_or("?");
                let _ = tx.send(Msg::Progress(
                    (i + 1) as f32 / total as f32,
                    format!("Обработка ({}/{}): {}", i + 1, total, name),
                ));
                match remover.process_file(file, None) {
                    Ok(_) => ok += 1,
                    Err(e) => errors.push(format!("{}: {}", name, e)),
                }
            }
            let _ = tx.send(Msg::Done { ok, total, errors });
        });

        self.rx = Some(rx);
        self.busy = true;
        self.progress = 0.0;
        self.status = "Обработка...".to_string();
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut pending = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                pending.push(msg);
            }
        }
        for msg in pending {
                match msg {
                    Msg::Progress(p, s) => {
                        self.progress = p;
                        self.status = s;
                    }
                    Msg::Cancelled => {
                        self.busy = false;
                        self.progress = 0.0;
                        self.status = "Отменено".to_string();
                        self.rx = None;
                    }
                    Msg::Done { ok, total, errors } => {
                        self.busy = false;
                        self.progress = 0.0;
                        self.status = format!("Готово: {} из {}", ok, total);
                        self.rx = None;
                        let mut text = format!("Обработано успешно: {} из {}", ok, total);
                        if !errors.is_empty() {
                            text.push_str(&format!("\n\nОшибки ({}):\n", errors.len()));
                            for e in errors.iter().take(5) {
                                text.push_str(&format!("{}\n", e));
                            }
                            if errors.len() > 5 {
                                text.push_str(&format!("\n... и ещё {}", errors.len() - 5));
                            }
                        }
                        MessageDialog::new()
                            .set_title("Готово")
                            .set_description(&text)
                            .show();
                    }
                }
        }
        if self.busy {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(12.0);
                ui.heading("Удалитель Водяных Знаков v8.0");
                ui.label(format!("Rust-порт • образцов загружено: {}", self.remover.templates.len()));
            });

            ui.separator();
            ui.horizontal(|ui| {
                ui.label("База образцов:");
                ui.label(self.samples_dir.to_string_lossy());
                if ui.add_enabled(!self.busy, egui::Button::new("Сменить…")).clicked() {
                    self.choose_samples();
                }
            });
            ui.add_space(8.0);
            ui.label("Выберите изображения или папку. Результат сохранится рядом с оригиналом (*_cleaned).");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui.add_enabled(!self.busy, egui::Button::new("📁 Выбрать файлы")).clicked() {
                    self.start("files");
                }
                if ui.add_enabled(!self.busy, egui::Button::new("📂 Выбрать папку")).clicked() {
                    self.start("folder");
                }
                if ui
                    .add_enabled(self.busy, egui::Button::new("✖ Отменить"))
                    .clicked()
                {
                    self.cancel.store(true, Ordering::SeqCst);
                    self.status = "Отмена...".to_string();
                }
            });

            ui.add_space(12.0);
            ui.add(egui::ProgressBar::new(self.progress).show_percentage());
            ui.add_space(6.0);
            ui.label(&self.status);
        });
    }
}

pub fn run(remover: WatermarkRemover, samples_dir: Option<&std::path::Path>) -> Result<()> {
    let dir = samples_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| remover.samples_dir.clone());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Удалитель Водяных Знаков v8.0 (Rust)")
            .with_inner_size([640.0, 340.0])
            .with_resizable(false),
        ..Default::default()
    };

    eframe::run_native(
        "delete-watermark",
        options,
        Box::new(move |_cc| Ok(Box::new(App::new(remover, dir)))),
    )
    .map_err(|e| anyhow::anyhow!("Ошибка GUI: {}", e))
}
