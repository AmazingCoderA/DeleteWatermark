//! Операции с изображениями: shades of gray, ресайз, маски и inpaint.
//! Реализовано на чистом Rust, чтобы проект собирался без системного OpenCV
//! (в том числе под Windows / Linux / Android).

use image::{DynamicImage, GrayImage, ImageBuffer, Luma, RgbImage};

/// Полутоновое изображение с плавающей точкой (аналог CV_32FC1).
pub type GrayF32 = ImageBuffer<Luma<f32>, Vec<f32>>;

/// Перевод в оттенки серого по формуле OpenCV COLOR_BGR2GRAY:
/// Y = 0.299*R + 0.587*G + 0.114*B (альфа игнорируется, как в cv2.imread).
pub fn to_gray_f32(img: &DynamicImage) -> GrayF32 {
    let (w, h) = (img.width(), img.height());
    let mut out = GrayF32::new(w, h);

    // Альфа-канал отбрасывается, как это делает cv2.imread(..., IMREAD_GRAYSCALE).
    let rgba = img.to_rgba8();
    for (i, dst) in out.as_mut().iter_mut().enumerate() {
        let p = &rgba.as_raw()[i * 4..];
        *dst = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
    }
    out
}

/// Билинейный ресайз, совместимый с cv2.resize(... INTER_LINEAR):
/// исходная координата = (dst + 0.5) * scale - 0.5.
pub fn resize_gray(src: &GrayF32, nw: u32, nh: u32) -> GrayF32 {
    if nw == 0 || nh == 0 {
        return GrayF32::new(nw, nh);
    }
    let (sw, sh) = (src.width(), src.height());
    if nw == sw && nh == sh {
        return src.clone();
    }
    let mut out = GrayF32::new(nw, nh);
    let sx = sw as f64 / nw as f64;
    let sy = sh as f64 / nh as f64;

    let src_raw = src.as_raw();
    let out_raw = out.as_mut();

    for oy in 0..nh as usize {
        let fy = ((oy as f64 + 0.5) * sy - 0.5).clamp(0.0, sh as f64 - 1.0);
        let y0 = fy.floor() as usize;
        let y1 = (y0 + 1).min(sh as usize - 1);
        let ay = fy - y0 as f64;

        for ox in 0..nw as usize {
            let fx = ((ox as f64 + 0.5) * sx - 0.5).clamp(0.0, sw as f64 - 1.0);
            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(sw as usize - 1);
            let ax = fx - x0 as f64;

            let v00 = src_raw[y0 * sw as usize + x0] as f64;
            let v10 = src_raw[y0 * sw as usize + x1] as f64;
            let v01 = src_raw[y1 * sw as usize + x0] as f64;
            let v11 = src_raw[y1 * sw as usize + x1] as f64;

            let top = v00 * (1.0 - ax) + v10 * ax;
            let bot = v01 * (1.0 - ax) + v11 * ax;
            out_raw[oy * nw as usize + ox] = (top * (1.0 - ay) + bot * ay) as f32;
        }
    }
    out
}

/// Уменьшение изображения усреднением блоков d×d (для пирамиды поиска).
pub fn downscale_gray(src: &GrayF32, d: u32) -> GrayF32 {
    if d <= 1 {
        return src.clone();
    }
    let (nw, nh) = (src.width() / d, src.height() / d);
    let mut out = GrayF32::new(nw, nh);
    let inv = 1.0 / (d * d) as f32;
    for y in 0..nh {
        for x in 0..nw {
            let mut sum = 0f32;
            for dy in 0..d {
                for dx in 0..d {
                    sum += src.get_pixel(x * d + dx, y * d + dy).0[0];
                }
            }
            out.put_pixel(x, y, Luma([sum * inv]));
        }
    }
    out
}

/// Маска «чернил» шаблона: порог Оцу отделяет сам знак от фона,
/// затем 1px-дилатация захватывает сглаженные края букв.
/// Это позволяет удалить именно водяной знак, не задевая соседний текст.
pub fn otsu_ink_mask(gray: &GrayF32) -> Vec<u8> {
    let (w, h) = (gray.width() as usize, gray.height() as usize);
    let total = (w * h) as u32;
    let mut hist = [0u32; 256];
    for &v in gray.as_raw() {
        let b = (v.round() as i32).clamp(0, 255) as usize;
        hist[b] += 1;
    }
    let sum_all: f64 = (0..256).map(|i| i as f64 * hist[i] as f64).sum();

    let mut w_b = 0u32;
    let mut sum_b = 0f64;
    let mut best = -1f64;
    let mut thr = 127usize;
    for t in 0..256 {
        w_b += hist[t];
        if w_b == 0 {
            continue;
        }
        let w_f = total - w_b;
        if w_f == 0 {
            break;
        }
        sum_b += t as f64 * hist[t] as f64;
        let m_b = sum_b / w_b as f64;
        let m_f = (sum_all - sum_b) / w_f as f64;
        let between = w_b as f64 * w_f as f64 * (m_b - m_f) * (m_b - m_f);
        if between > best {
            best = between;
            thr = t;
        }
    }

    // Фон оцениваем по границе шаблона: он почти всегда светлый.
    let mut border_sum = 0f64;
    let mut border_n = 0usize;
    for x in 0..w {
        for y in [0, h - 1] {
            border_sum += gray.get_pixel(x as u32, y as u32).0[0] as f64;
            border_n += 1;
        }
    }
    for y in 0..h {
        for x in [0, w - 1] {
            border_sum += gray.get_pixel(x as u32, y as u32).0[0] as f64;
            border_n += 1;
        }
    }
    let bg = border_sum / border_n.max(1) as f64;

    let mut dark_sum = 0f64;
    let mut dark_n = 0usize;
    let mut light_sum = 0f64;
    let mut light_n = 0usize;
    for &v in gray.as_raw() {
        if (v.round() as i32) <= thr as i32 {
            dark_sum += v as f64;
            dark_n += 1;
        } else {
            light_sum += v as f64;
            light_n += 1;
        }
    }
    let dark_mean = dark_sum / dark_n.max(1) as f64;
    let light_mean = light_sum / light_n.max(1) as f64;
    // Знак — тот класс, который НЕ похож на фон по границе.
    let ink_is_dark = (bg - light_mean).abs() < (bg - dark_mean).abs();

    // Порог от фона захватывает и полупрозрачные сглаженные края букв,
    // чтобы после инпейнтинга не оставалось серого ореола.
    let edge = if ink_is_dark {
        (bg - 8.0).max(thr as f64 + 15.0).clamp(0.0, 255.0) as f32
    } else {
        (bg + 8.0).min(thr as f64 - 15.0).clamp(0.0, 255.0) as f32
    };
    let mut raw = vec![0u8; w * h];
    for (i, &v) in gray.as_raw().iter().enumerate() {
        let is_ink = if ink_is_dark { v <= edge } else { v >= edge };
        if is_ink {
            raw[i] = 255;
        }
    }
    let count = raw.iter().filter(|&&v| v != 0).count();
    if count > total as usize * 8 / 10 || count == 0 {
        // Резерв: без «размытия» порога, чтобы маска не съела весь кадр.
        for (i, &v) in gray.as_raw().iter().enumerate() {
            raw[i] = if (ink_is_dark && v <= thr as f32)
                || (!ink_is_dark && v > thr as f32)
            {
                255
            } else {
                0
            };
        }
    }
    // Две итерации: core-глиф + 2 px на сглаженные края (антиалиасинг).
    dilate3(&dilate3(&raw, w, h), w, h)
}

/// Дилатация маски квадратом 3x3 (эквивалент cv2.dilate с MORPH_RECT 3x3).
pub fn dilate3(mask: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; mask.len()];
    for y in 0..h {
        let y0 = y.saturating_sub(1);
        let y1 = (y + 1).min(h - 1);
        for x in 0..w {
            let x0 = x.saturating_sub(1);
            let x1 = (x + 1).min(w - 1);
            let mut v = 0u8;
            for yy in y0..=y1 {
                for xx in x0..=x1 {
                    v = v.max(mask[yy * w + xx]);
                }
            }
            out[y * w + x] = v;
        }
    }
    out
}

/// L1-преобразование расстояния до ближайшего «чистого» пикселя.
/// Два прохода дают точный результат для 4-связности (как cv2.distanceTransform DIST_L1).
pub fn l1_distance(dilated_mask: &[u8], w: usize, h: usize) -> Vec<u32> {
    const INF: u32 = u32::MAX / 4;
    let mut dist = vec![INF; w * h];
    for i in 0..w * h {
        if dilated_mask[i] == 0 {
            dist[i] = 0;
        }
    }
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if x > 0 {
                dist[i] = dist[i].min(dist[i - 1] + 1);
            }
            if y > 0 {
                dist[i] = dist[i].min(dist[i - w] + 1);
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            if x + 1 < w {
                dist[i] = dist[i].min(dist[i + 1] + 1);
            }
            if y + 1 < h {
                dist[i] = dist[i].min(dist[i + w] + 1);
            }
        }
    }
    dist
}

/// Inpainting в духе алгоритма Телеа (OpenCV INPAINT_TELEA):
/// пиксели восстанавливаются от границы маски внутрь, с учётом
/// направления градиента и расстояния до границы.
pub fn inpaint_telea(src: &RgbImage, mask: &[u8], radius: i32) -> RgbImage {
    let (w, h) = (src.width() as usize, src.height() as usize);
    if w == 0 || h == 0 || radius <= 0 {
        return src.clone();
    }

    let dilated = dilate3(mask, w, h);
    let dist = l1_distance(&dilated, w, h);

    let mut order: Vec<usize> = (0..w * h).filter(|&i| mask[i] != 0).collect();
    order.sort_unstable_by_key(|&i| dist[i]);

    let src_raw = src.as_raw();
    let mut color: Vec<[f32; 3]> = vec![[0.0; 3]; w * h];
    let mut known = vec![true; w * h];
    for i in 0..w * h {
        for c in 0..3 {
            color[i][c] = src_raw[i * 3 + c] as f32;
        }
        if mask[i] != 0 {
            known[i] = false;
        }
    }

    let sample = |colors: &[[f32; 3]], known: &[bool], x: i32, y: i32, c: usize, cx: usize| -> f32 {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            return colors[cx][c];
        }
        let i = y as usize * w + x as usize;
        if known[i] {
            colors[i][c]
        } else {
            colors[cx][c]
        }
    };

    // Обрабатываем пиксели маски в порядке увеличения расстояния до границы.
    for &idx in &order {
        let cx = idx % w;
        let cy = idx / w;
        let mut sw_sum = [0f32; 3];
        let mut plain_sum = [0f32; 3];
        let mut flat_min = [255f32; 3];
        let mut flat_max = [0f32; 3];
        let mut w_sum = 0f32;

        let r = radius.clamp(1, 9);
        for dy in -r..=r {
            for dx in -r..=r {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = cx as i32 + dx;
                let ny = cy as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let ni = ny as usize * w + nx as usize;
                if !known[ni] {
                    continue;
                }

                let de = ((dx * dx + dy * dy) as f32).sqrt();
                let w_geo = 1.0 / de;
                let w_dst = 1.0 / ((dist[ni] + 1) as f32).powi(2);

                let mut grad = [[0f32; 2]; 3];
                for c in 0..3 {
                    grad[c][0] = 0.5
                        * (sample(&color, &known, nx + 1, ny, c, ni)
                            - sample(&color, &known, nx - 1, ny, c, ni));
                    grad[c][1] = 0.5
                        * (sample(&color, &known, nx, ny + 1, c, ni)
                            - sample(&color, &known, nx, ny - 1, c, ni));
                }
                let glx = 0.299 * grad[0][0] + 0.587 * grad[1][0] + 0.114 * grad[2][0];
                let gly = 0.299 * grad[0][1] + 0.587 * grad[1][1] + 0.114 * grad[2][1];
                let glen = (glx * glx + gly * gly).sqrt();
                let w_dir = if glen > 1e-3 {
                    // синус угла между градиентом и направлением на пиксель:
                    // больше вес у соседей вдоль линии (изофоты).
                    let sin_angle = (glx * dy as f32 - gly * dx as f32).abs() / (glen * de);
                    0.05 + sin_angle
                } else {
                    1.0
                };

                let weight = w_geo * w_dst * w_dir;
                for c in 0..3 {
                    let v = color[ni][c];
                    flat_min[c] = flat_min[c].min(v);
                    flat_max[c] = flat_max[c].max(v);
                    plain_sum[c] += weight * v;
                    let predicted = v + grad[c][0] * dx as f32 + grad[c][1] * dy as f32;
                    sw_sum[c] += weight * predicted;
                }
                w_sum += weight;
            }
        }

        if w_sum > 1e-9 {
            // На однотонном фоне градиентная экстраполяция даёт лёгкий
            // серый ореол — используем простое среднее соседей.
            let flat = (0..3).all(|c| flat_max[c] - flat_min[c] < 3.0);
            for c in 0..3 {
                let v = if flat { plain_sum[c] } else { sw_sum[c] } / w_sum;
                color[idx][c] = v.clamp(0.0, 255.0);
            }
        }
        known[idx] = true;
    }

    let mut out = RgbImage::new(w as u32, h as u32);
    for (i, px) in out.as_mut().chunks_exact_mut(3).enumerate() {
        for c in 0..3 {
            px[c] = color[i][c].round() as u8;
        }
    }
    // Маска могла быть больше radius — сохраняем исходные пиксели вне неё.
    for i in 0..w * h {
        if mask[i] == 0 {
            for c in 0..3 {
                out.as_mut()[i * 3 + c] = src_raw[i * 3 + c];
            }
        }
    }
    out
}

/// Маска с закрашенными прямоугольниками (fallback, если маска знака недоступна).
pub fn fill_rect(mask: &mut GrayImage, x0: i32, y0: i32, x1: i32, y1: i32, value: u8) {
    let w = mask.width() as i32;
    let h = mask.height() as i32;
    let xa = x0.max(0);
    let ya = y0.max(0);
    let xb = x1.min(w - 1);
    let yb = y1.min(h - 1);
    for y in ya..=yb {
        for x in xa..=xb {
            mask.put_pixel(x as u32, y as u32, Luma([value]));
        }
    }
}

/// Накладывает пиксельную маску знака в позицию detections.
pub fn paste_mask(dst: &mut GrayImage, stamp: &[u8], sw: usize, sh: usize, ox: i32, oy: i32) {
    let (dw, dh) = (dst.width() as i32, dst.height() as i32);
    for dy in 0..sh {
        let y = oy + dy as i32;
        if y < 0 || y >= dh {
            continue;
        }
        for dx in 0..sw {
            if stamp[dy * sw + dx] == 0 {
                continue;
            }
            let x = ox + dx as i32;
            if x < 0 || x >= dw {
                continue;
            }
            dst.put_pixel(x as u32, y as u32, Luma([255]));
        }
    }
}
