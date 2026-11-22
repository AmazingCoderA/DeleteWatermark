//! Красивый десктопный GUI (egui/eframe) — основной режим по умолчанию.

use anyhow::Result;
use delete_watermark_android::config::{is_supported, EngineConfig};
use delete_watermark_android::engine::WatermarkRemover;
use eframe::egui::{self, Color32, FontId, RichText, Shadow, TextStyle};
use rfd::{FileDialog, MessageDialog};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

const BLUE: Color32 = Color32::from_rgb(102, 126, 234);
const PURPLE: Color32 = Color32::from_rgb(118, 75, 162);
const RED: Color32 = Color32::from_rgb(211, 47, 47);
const SOFT: Color32 = Color32::from_rgb(238, 242, 255);
const INK: Color32 = Color32::from_rgb(40, 44, 70);
const MUTED: Color32 = Color32::from_rgb(120, 126, 150);

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

fn setup_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals.dark_mode = false;
    style.visuals.window_fill = SOFT;
    style.visuals.panel_fill = SOFT;
    style.visuals.text_cursor.stroke.color = BLUE;
    style.visuals.selection.bg_fill = BLUE;
    style.visuals.widgets.inactive.bg_fill = BLUE;
    style.visuals.widgets.inactive.fg_stroke.color = Color32::WHITE;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(85, 104, 211);
    style.visuals.widgets.hovered.fg_stroke.color = Color32::WHITE;
    style.visuals.widgets.active.bg_fill = PURPLE;
    style.visuals.widgets.active.fg_stroke.color = Color32::WHITE;
    style.visuals.widgets.noninteractive.bg_fill = Color32::WHITE;
    style.visuals.widgets.noninteractive.fg_stroke.color = INK;

    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(26.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Body,
        FontId::new(14.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(15.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(11.5, egui::FontFamily::Proportional),
    );
    ctx.set_style(style);
}

impl App {
    fn new(remover: WatermarkRemover, samples_dir: PathBuf) -> Self {
        let config = remover.config.clone();
        let count = remover.templates.len();
        Self {
            remover: Arc::new(remover),
            config,
            samples_dir,
            status: format!("Готов к работе • загружено образцов: {}", count),
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
                    self.status = format!("База образцов обновлена • {} шт.", r.templates.len());
                    self.samples_dir = dir;
                    self.remover = Arc::new(r);
                }
                Err(e) => self.status = format!("Ошибка загрузки базы: {}", e),
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
                    format!("Обработка {}/{} • {}", i + 1, total, name),
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
        self.status = "Обработка началась…".to_string();
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        setup_style(ctx);

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
                    self.status = "Обработка отменена".to_string();
                    self.rx = None;
                }
                Msg::Done { ok, total, errors } => {
                    self.busy = false;
                    self.progress = 1.0;
                    self.status = format!("Готово • успешно {} из {}", ok, total);
                    self.rx = None;
                    let mut text = format!("Обработано успешно: {} из {}", ok, total);
                    if !errors.is_empty() {
                        text.push_str(&format!("\n\nОшибки ({}):\n", errors.len()));
                        for e in errors.iter().take(5) {
                            text.push_str(&format!("{}\n", e));
                        }
                        if errors.len() > 5 {
                            text.push_str(&format!("\n… и ещё {}", errors.len() - 5));
                        }
                    }
                    MessageDialog::new()
                        .set_title("Обработка завершена")
                        .set_description(&text)
                        .show();
                }
            }
        }
        if self.busy {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        // Шапка
        egui::TopBottomPanel::top("header")
            .exact_height(116.0)
            .frame(egui::Frame::new().fill(BLUE).inner_margin(egui::Margin {
                left: 32,
                right: 32,
                top: 22,
                bottom: 18,
            }))
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("🎨  Удалитель водяных знаков")
                            .color(Color32::WHITE)
                            .size(27.0)
                            .strong(),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Точное удаление по базе образцов  •  Rust v8.0")
                            .color(Color32::from_white_alpha(225))
                            .size(14.0),
                    );
                });
            });

        // Основная карточка
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(SOFT))
            .show(ctx, |ui| {
                let card = egui::Frame::new()
                    .fill(Color32::WHITE)
                    .corner_radius(18.0)
                    .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(226, 230, 245)))
                    .shadow(Shadow {
                        offset: [0, 8],
                        blur: 20,
                        spread: 0,
                        color: Color32::from_black_alpha(35),
                    })
                    .inner_margin(egui::Margin {
                        left: 28,
                        right: 28,
                        top: 26,
                        bottom: 26,
                    })
                    .outer_margin(egui::Margin {
                        left: 24,
                        right: 24,
                        top: 24,
                        bottom: 12,
                    });
                card.show(ui, |ui| {
                    ui.label(
                        RichText::new("БАЗА ОБРАЗЦОВ")
                            .color(MUTED)
                            .size(11.0)
                            .strong(),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let path = self.samples_dir.to_string_lossy();
                        let short = if path.chars().count() > 52 {
                            format!("…{}", path.chars().skip(path.chars().count() - 51).collect::<String>())
                        } else {
                            path.to_string()
                        };
                        ui.label(RichText::new(short).monospace().size(13.0).color(INK));
                        ui.label(
                            RichText::new(format!("{} шт.", self.remover.templates.len()))
                                .color(BLUE)
                                .strong()
                                .size(13.0),
                        );
                        let change = egui::Button::new(
                            RichText::new("Сменить").color(BLUE).size(13.0).strong(),
                        )
                        .fill(SOFT)
                        .stroke(egui::Stroke::new(1.0_f32, BLUE))
                        .corner_radius(8.0);
                        if ui.add_enabled(!self.busy, change).clicked() {
                            self.choose_samples();
                        }
                    });

                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(14.0);

                    ui.label(
                        RichText::new("ВЫБЕРИТЕ ИЗОБРАЖЕНИЯ")
                            .color(MUTED)
                            .size(11.0)
                            .strong(),
                    );
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        let b1 = egui::Button::new(
                            RichText::new("📁  Выбрать файлы")
                                .color(Color32::WHITE)
                                .size(16.0)
                                .strong(),
                        )
                        .fill(BLUE)
                        .corner_radius(10.0)
                        .min_size(egui::vec2(248.0, 54.0));
                        if ui.add_enabled(!self.busy, b1).clicked() {
                            self.start("files");
                        }

                        let b2 = egui::Button::new(
                            RichText::new("📂  Выбрать папку")
                                .color(Color32::WHITE)
                                .size(16.0)
                                .strong(),
                        )
                        .fill(PURPLE)
                        .corner_radius(10.0)
                        .min_size(egui::vec2(248.0, 54.0));
                        if ui.add_enabled(!self.busy, b2).clicked() {
                            self.start("folder");
                        }

                        let b3 = egui::Button::new(
                            RichText::new("✖  Отмена")
                                .color(Color32::WHITE)
                                .size(14.0)
                                .strong(),
                        )
                        .fill(RED)
                        .corner_radius(10.0)
                        .min_size(egui::vec2(120.0, 54.0));
                        if ui.add_enabled(self.busy, b3).clicked() {
                            self.cancel.store(true, Ordering::SeqCst);
                            self.status = "Отмена…".to_string();
                        }
                    });

                    ui.add_space(22.0);
                    ui.separator();
                    ui.add_space(14.0);

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("ПРОГРЕСС").color(MUTED).size(11.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{}%", (self.progress * 100.0) as i32))
                                    .color(BLUE)
                                    .strong()
                                    .size(13.0),
                            );
                        });
                    });
                    ui.add_space(6.0);
                    let bar_w = ui.available_width();
                    let (bar_rect, _) = ui.allocate_exact_size(
                        egui::vec2(bar_w, 12.0),
                        egui::Sense::hover(),
                    );
                    let painter = ui.painter();
                    painter.rect_filled(
                        bar_rect,
                        6.0_f32,
                        Color32::from_rgb(226, 232, 250),
                    );
                    if self.progress > 0.0 {
                        let mut filled = bar_rect;
                        filled.set_width(bar_rect.width() * self.progress.min(1.0));
                        painter.rect_filled(filled, 6.0_f32, BLUE);
                    }
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(&self.status)
                            .color(if self.busy { INK } else { MUTED })
                            .size(13.0),
                    );
                });
            });

        egui::TopBottomPanel::bottom("footer")
            .exact_height(34.0)
            .frame(egui::Frame::new().fill(SOFT))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Результат сохраняется рядом с оригиналом: *_cleaned.*")
                            .color(MUTED)
                            .size(11.5),
                    );
                });
            });
    }
}

pub fn run(remover: WatermarkRemover, samples_dir: Option<&Path>) -> Result<()> {
    let dir = samples_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| remover.samples_dir.clone());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Удалитель водяных знаков v8.0")
            .with_inner_size([780.0, 560.0])
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
