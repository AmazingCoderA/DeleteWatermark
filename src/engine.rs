//! Ядро обработки — точный порт класса WatermarkRemover из DeleteGdzSuper6.py.

use crate::config::{is_supported, EngineConfig};
use crate::image_ops::{
    fill_rect, inpaint_telea, otsu_ink_mask, paste_mask, resize_gray, to_gray_f32, GrayF32,
};
use crate::matcher::{Detection, FftMatcher, Integral};
use anyhow::{anyhow, Context, Result};
use image::{DynamicImage, GrayImage, RgbImage};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct Template {
    pub name: String,
    pub gray: GrayF32,
}

pub struct ProcessedImage {
    pub image: RgbImage,
    pub detections: usize,
}

pub struct WatermarkRemover {
    pub templates: Vec<Template>,
    pub samples_dir: PathBuf,
    pub config: EngineConfig,
}

impl WatermarkRemover {
    pub fn new(samples_dir: &Path, config: EngineConfig) -> Result<Self> {
        if !samples_dir.is_dir() {
            return Err(anyhow!("Папка '{}' не существует", samples_dir.display()));
        }
        let mut templates = Vec::new();
        let mut entries: Vec<_> = fs::read_dir(samples_dir)
            .with_context(|| format!("Не удалось прочитать '{}'", samples_dir.display()))?
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let path = entry.path();
            if !path.is_file() || !is_supported(&path) {
                continue;
            }
            if let Ok(img) = image::open(&path) {
                templates.push(Template {
                    name: path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    gray: to_gray_f32(&img),
                });
            }
        }
        if templates.is_empty() {
            return Err(anyhow!(
                "В папке '{}' нет образцов водяных знаков",
                samples_dir.display()
            ));
        }
        Ok(Self {
            templates,
            samples_dir: samples_dir.to_path_buf(),
            config,
        })
    }

    /// scale-ы как np.linspace(min, max, steps)
    fn scales(&self) -> Vec<f32> {
        let steps = self.config.scale_steps.max(1);
        if steps == 1 {
            return vec![self.config.scale_min];
        }
        (0..steps)
            .map(|i| {
                self.config.scale_min
                    + i as f32 * (self.config.scale_max - self.config.scale_min)
                        / (steps - 1) as f32
            })
            .collect()
    }

    /// Возвращает уникальные размеры шаблона для всех масштабов, которые влезают в кадр.
    fn scaled_sizes(&self, tpl: &Template, iw: u32, ih: u32) -> Vec<(u32, u32)> {
        let mut seen = Vec::new();
        for s in self.scales() {
            let w = (tpl.gray.width() as f32 * s) as u32;
            let h = (tpl.gray.height() as f32 * s) as u32;
            if w == 0 || h == 0 || w > iw || h > ih {
                continue;
            }
            if !seen.contains(&(w, h)) {
                seen.push((w, h));
            }
        }
        seen
    }

    /// Точная проверка кандидата в полном разрешении (NCC в небольшом окне).
    fn refine_best(
        &self,
        image_gray: &GrayF32,
        integral: &Integral,
        tpl: &GrayF32,
        cx: u32,
        cy: u32,
        radius: i32,
    ) -> Option<Detection> {
        let (tw, th) = (tpl.width(), tpl.height());
        let (iw, ih) = (image_gray.width(), image_gray.height());
        let n = (tw * th) as f64;
        let (mut st, mut st2) = (0f64, 0f64);
        for &v in tpl.as_raw() {
            st += v as f64;
            st2 += (v as f64) * (v as f64);
        }
        let mt = st / n;
        let vt = st2 / n - mt * mt;
        if vt <= 1e-6 {
            return None;
        }

        let mut best: Option<Detection> = None;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let x = cx as i32 + dx;
                let y = cy as i32 + dy;
                if x < 0 || y < 0 || x + tw as i32 > iw as i32 || y + th as i32 > ih as i32 {
                    continue;
                }
                let (si, si2) = integral.window(x as usize, y as usize, tw as usize, th as usize);
                let mi = si / n;
                let vi = si2 / n - mi * mi;
                if vi <= 1e-6 {
                    continue;
                }
                let mut cross = 0f64;
                for ty in 0..th {
                    for tx in 0..tw {
                        cross += (tpl.get_pixel(tx, ty).0[0] as f64)
                            * (image_gray.get_pixel(x as u32 + tx, y as u32 + ty).0[0] as f64);
                    }
                }
                let score = ((cross / n - mt * mi) / (vt * vi).sqrt()) as f32;
                if score >= self.config.match_threshold
                    && best.as_ref().map_or(true, |b| score > b.score)
                {
                    best = Some(Detection {
                        x: x as u32,
                        y: y as u32,
                        w: tw,
                        h: th,
                        score,
                        ink: None,
                    });
                }
            }
        }
        best
    }

    fn find_watermarks(
        &self,
        full_gray: &GrayF32,
        full_integral: &Integral,
        search_integral: &Integral,
        matcher: &mut FftMatcher,
    ) -> Vec<Detection> {
        let (iw, ih) = (full_gray.width(), full_gray.height());
        let d = self.config.pyramid.max(1);
        let coarse_thr = (self.config.match_threshold - 0.12).max(0.25);
        let mut all = Vec::new();

        for tpl in &self.templates {
            let sizes = self.scaled_sizes(tpl, iw, ih);
            for (tw, th) in sizes {
                let full_tpl = resize_gray(&tpl.gray, tw, th);
                // Маска именно знака (а не прямоугольника) — соседний текст не трогаем.
                let ink = Arc::new(otsu_ink_mask(&full_tpl));
                if d == 1 {
                    for mut det in
                        matcher.find(search_integral, &full_tpl, self.config.match_threshold)
                    {
                        det.ink = Some(Arc::clone(&ink));
                        all.push(det);
                    }
                    continue;
                }
                let sw = (tw / d).max(2);
                let sh = (th / d).max(2);
                let small_tpl = resize_gray(&tpl.gray, sw, sh);
                let mut candidates = matcher.find(search_integral, &small_tpl, coarse_thr);
                if candidates.is_empty() {
                    continue;
                }
                candidates.sort_by(|a, b| {
                    b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
                });
                let radius = (d as i32) * 2;
                for c in candidates.into_iter().take(120) {
                    if let Some(mut det) = self.refine_best(
                        full_gray,
                        full_integral,
                        &full_tpl,
                        c.x * d,
                        c.y * d,
                        radius,
                    ) {
                        det.ink = Some(Arc::clone(&ink));
                        all.push(det);
                    }
                }
            }
        }
        all
    }

    /// Non-maximum suppression (аналог _non_max_suppression в Python).
    fn non_max_suppression(&self, detections: Vec<Detection>) -> Vec<Detection> {
        if detections.is_empty() {
            return Vec::new();
        }
        let mut order: Vec<usize> = (0..detections.len()).collect();
        order.sort_by(|&a, &b| {
            detections[a]
                .score
                .partial_cmp(&detections[b].score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut keep = Vec::new();
        while !order.is_empty() {
            let i = *order.last().unwrap();
            keep.push(i);
            let x1_i = detections[i].x as i64;
            let y1_i = detections[i].y as i64;
            let x2_i = x1_i + detections[i].w as i64;
            let y2_i = y1_i + detections[i].h as i64;

            let mut rest = Vec::new();
            for &j in &order[..order.len() - 1] {
                let x1_j = detections[j].x as i64;
                let y1_j = detections[j].y as i64;
                let x2_j = x1_j + detections[j].w as i64;
                let y2_j = y1_j + detections[j].h as i64;
                let iw = (x2_i.min(x2_j) - x1_i.max(x1_j)).max(0);
                let ih = (y2_i.min(y2_j) - y1_i.max(y1_j)).max(0);
                let area_j = (detections[j].w as i64) * (detections[j].h as i64);
                let overlap = if area_j > 0 {
                    (iw * ih) as f32 / area_j as f32
                } else {
                    0.0
                };
                if overlap <= self.config.overlap_threshold {
                    rest.push(j);
                }
            }
            order = rest;
        }
        keep.into_iter().map(|i| detections[i].clone()).collect()
    }

    pub fn process_dynamic(&self, img: &DynamicImage) -> Result<ProcessedImage> {
        let prof = std::env::var_os("DW_PROFILE").is_some();
        let t0 = std::time::Instant::now();
        let image_gray = to_gray_f32(img);
        let (iw, ih) = (image_gray.width(), image_gray.height());
        if prof { eprintln!("[prof] gray: {:?}", t0.elapsed()); }

        let full_integral = Integral::new(&image_gray);
        let d = self.config.pyramid.clamp(1, 4);
        let search_gray = crate::image_ops::downscale_gray(&image_gray, d);
        let search_integral = Integral::new(&search_gray);
        let mut matcher = FftMatcher::new(&search_gray);
        if prof { eprintln!("[prof] integral+fft setup: {:?}", t0.elapsed()); }
        let t_find = std::time::Instant::now();
        let found = self.find_watermarks(
            &image_gray,
            &full_integral,
            &search_integral,
            &mut matcher,
        );
        if prof {
            eprintln!("[prof] find_watermarks: {:?}", t_find.elapsed());
            if let Some(p) = &matcher.profile {
                eprintln!(
                    "[prof]   calls={} corr={:.2}s detect={:.2}s",
                    p.calls,
                    p.corr_ns as f64 / 1e9,
                    p.detect_ns as f64 / 1e9
                );
            }
        }
        if found.is_empty() {
            return Ok(ProcessedImage {
                image: img.to_rgb8(),
                detections: 0,
            });
        }
        let filtered = self.non_max_suppression(found);
        if filtered.is_empty() {
            return Ok(ProcessedImage {
                image: img.to_rgb8(),
                detections: 0,
            });
        }

        let mut mask = GrayImage::new(iw, ih);
        for d in &filtered {
            match &d.ink {
                Some(ink) => paste_mask(
                    &mut mask,
                    ink,
                    d.w as usize,
                    d.h as usize,
                    d.x as i32,
                    d.y as i32,
                ),
                None => {
                    let pad = self.config.mask_pad;
                    fill_rect(
                        &mut mask,
                        d.x as i32 - pad,
                        d.y as i32 - pad,
                        (d.x + d.w) as i32 + pad,
                        (d.y + d.h) as i32 + pad,
                        255,
                    );
                }
            }
        }
        let t_inpaint = std::time::Instant::now();
        let cleaned = inpaint_telea(
            &img.to_rgb8(),
            mask.as_raw(),
            self.config.inpaint_radius,
        );
        if prof { eprintln!("[prof] inpaint: {:?} (detections {})", t_inpaint.elapsed(), filtered.len()); }
        Ok(ProcessedImage {
            image: cleaned,
            detections: filtered.len(),
        })
    }

    /// Обработка файла с сохранением результата рядом (или в output_path).
    pub fn process_file(&self, input: &Path, output: Option<&Path>) -> Result<(String, PathBuf, usize)> {
        let img = image::open(input)
            .with_context(|| format!("Не удалось прочитать файл '{}'", input.display()))?;
        let result = self.process_dynamic(&img)?;
        if result.detections == 0 {
            return Ok((
                "Водяные знаки не найдены".to_string(),
                input.to_path_buf(),
                0,
            ));
        }
        let out_path = match output {
            Some(p) => p.to_path_buf(),
            None => {
                let stem = input
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("image");
                let ext = input
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("png");
                input.with_file_name(format!("{}_cleaned.{}", stem, ext))
            }
        };
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        DynamicImage::ImageRgb8(result.image)
            .save(&out_path)
            .with_context(|| format!("Не удалось сохранить '{}'", out_path.display()))?;
        let msg = if result.detections > 0 {
            format!("Удалено {} водяных знаков", result.detections)
        } else {
            "Водяные знаки не найдены".to_string()
        };
        Ok((msg, out_path, result.detections))
    }

    /// Обработка байтов (для веб-интерфейса): возвращает PNG и число удалений.
    pub fn process_bytes(&self, data: &[u8]) -> Result<(Vec<u8>, usize)> {
        let img = image::load_from_memory(data).context("Не удалось прочитать изображение")?;
        let result = self.process_dynamic(&img)?;
        if result.detections == 0 {
            return Ok((Vec::new(), 0));
        }
        let mut buf = Vec::new();
        DynamicImage::ImageRgb8(result.image)
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)?;
        Ok((buf, result.detections))
    }

    pub fn info(&self) -> String {
        format!(
            "Загружено образцов: {} (папка: {})",
            self.templates.len(),
            self.samples_dir.display()
        )
    }
}
