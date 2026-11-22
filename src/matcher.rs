//! Быстрый поиск шаблона TM_CCOEFF_NORMED через FFT + интегральные изображения.
//! Математически эквивалентно cv2.matchTemplate(image, templ, cv2.TM_CCOEFF_NORMED),
//! но не требует OpenCV. FFT-спектр изображения кэшируется по размерам шаблона.

use crate::image_ops::GrayF32;
use rayon::prelude::*;
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Detection {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub score: f32,
    /// По пиксельная маска знака (из шаблона) — чтобы не трогать соседний текст.
    pub ink: Option<Arc<Vec<u8>>>,
}

fn transpose(src: &[Complex<f32>], dst: &mut [Complex<f32>], w: usize, h: usize) {
    dst.par_chunks_mut(h).enumerate().for_each(|(x, col)| {
        for y in 0..h {
            col[y] = src[y * w + x];
        }
    });
}

fn fft2(
    buf: &mut [Complex<f32>],
    tmp: &mut [Complex<f32>],
    pw: usize,
    ph: usize,
    row: &Arc<dyn Fft<f32>>,
    col: &Arc<dyn Fft<f32>>,
    row_scratch: usize,
    col_scratch: usize,
) {
    let mut rs = vec![Complex::new(0f32, 0f32); row_scratch];
    for r in buf.chunks_mut(pw) {
        row.process_with_scratch(r, &mut rs);
    }
    transpose(buf, tmp, pw, ph);
    let mut cs = vec![Complex::new(0f32, 0f32); col_scratch];
    for r in tmp.chunks_mut(ph) {
        col.process_with_scratch(r, &mut cs);
    }
    transpose(tmp, buf, ph, pw);
}

/// Интегральные изображения (f64 — важна точность дисперсии на однотонных участках).
pub struct Integral {
    sum: Vec<f64>,
    sq: Vec<f64>,
    w: usize,
}

impl Integral {
    pub fn new(gray: &GrayF32) -> Self {
        let w = gray.width() as usize;
        let h = gray.height() as usize;
        let stride = w + 1;
        let mut sum = vec![0f64; stride * (h + 1)];
        let mut sq = vec![0f64; stride * (h + 1)];
        let raw = gray.as_raw();

        for y in 0..h {
            let mut row_sum = 0f64;
            let mut row_sq = 0f64;
            for x in 0..w {
                let v = raw[y * w + x] as f64;
                row_sum += v;
                row_sq += v * v;
                sum[(y + 1) * stride + x + 1] = sum[y * stride + x + 1] + row_sum;
                sq[(y + 1) * stride + x + 1] = sq[y * stride + x + 1] + row_sq;
            }
        }
        Self { sum, sq, w }
    }

    #[inline]
    pub fn window(&self, x: usize, y: usize, ww: usize, hh: usize) -> (f64, f64) {
        let s = self.w + 1;
        let a = y * s + x;
        let b = y * s + x + ww;
        let c = (y + hh) * s + x;
        let d = (y + hh) * s + x + ww;
        (
            self.sum[d] - self.sum[b] - self.sum[c] + self.sum[a],
            self.sq[d] - self.sq[b] - self.sq[c] + self.sq[a],
        )
    }
}

/// Всё необходимое для корреляции при конкретном размере FFT.
struct Group {
    pw: usize,
    spectrum: Vec<Complex<f32>>,
    work: Vec<Complex<f32>>,
    tmp: Vec<Complex<f32>>,
    row_fwd: Arc<dyn Fft<f32>>,
    col_fwd: Arc<dyn Fft<f32>>,
    row_inv: Arc<dyn Fft<f32>>,
    col_inv: Arc<dyn Fft<f32>>,
    row_scratch: usize,
    col_scratch: usize,
}

impl Group {
    fn new(image: &GrayF32, pw: usize, ph: usize) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let row_fwd = planner.plan_fft_forward(pw);
        let col_fwd = planner.plan_fft_forward(ph);
        let row_inv = planner.plan_fft_inverse(pw);
        let col_inv = planner.plan_fft_inverse(ph);
        let row_scratch = row_fwd.get_inplace_scratch_len().max(row_inv.get_inplace_scratch_len());
        let col_scratch = col_fwd.get_inplace_scratch_len().max(col_inv.get_inplace_scratch_len());

        let mut spectrum = vec![Complex::new(0f32, 0f32); pw * ph];
        for (y, row) in image.as_raw().chunks(image.width() as usize).enumerate() {
            for (x, &v) in row.iter().enumerate() {
                spectrum[y * pw + x] = Complex::new(v, 0.0);
            }
        }
        let mut tmp = vec![Complex::new(0f32, 0f32); pw * ph];
        fft2(
            &mut spectrum,
            &mut tmp,
            pw,
            ph,
            &row_fwd,
            &col_fwd,
            row_scratch,
            col_scratch,
        );

        Self {
            pw,
            spectrum,
            work: vec![Complex::new(0f32, 0f32); pw * ph],
            tmp,
            row_fwd,
            col_fwd,
            row_inv,
            col_inv,
            row_scratch,
            col_scratch,
        }
    }

    fn correlate(&mut self, templ: &GrayF32) {
        self.work.fill(Complex::new(0f32, 0f32));
        let tw = templ.width() as usize;
        for (y, row) in templ.as_raw().chunks(tw).enumerate() {
            for (x, &v) in row.iter().enumerate() {
                self.work[y * self.pw + x] = Complex::new(v, 0.0);
            }
        }
        let ph = self.ph();
        fft2(
            &mut self.work,
            &mut self.tmp,
            self.pw,
            ph,
            &self.row_fwd,
            &self.col_fwd,
            self.row_scratch,
            self.col_scratch,
        );
        let spectrum = &self.spectrum;
        self.work
            .par_iter_mut()
            .zip(spectrum.par_iter())
            .for_each(|(w, s)| *w = *s * w.conj());
        fft2(
            &mut self.work,
            &mut self.tmp,
            self.pw,
            ph,
            &self.row_inv,
            &self.col_inv,
            self.row_scratch,
            self.col_scratch,
        );
        let norm = 1.0 / (self.pw * ph) as f32;
        self.work.par_iter_mut().for_each(|v| *v *= norm);
    }

    fn ph(&self) -> usize {
        self.tmp.len() / self.pw
    }
}

/// Менеджер FFT-групп для одного изображения.
#[derive(Default)]
pub struct MatcherProfile {
    pub calls: usize,
    pub corr_ns: u128,
    pub detect_ns: u128,
}

pub struct FftMatcher {
    image: GrayF32,
    groups: HashMap<(usize, usize), Group>,
    pub profile: Option<MatcherProfile>,
}

impl FftMatcher {
    pub fn new(image: &GrayF32) -> Self {
        let profile = std::env::var_os("DW_PROFILE").is_some().then(MatcherProfile::default);
        Self {
            image: image.clone(),
            groups: HashMap::new(),
            profile,
        }
    }

    /// Полный цикл: корреляция + сбор обнаружений для одного шаблона.
    pub fn find(
        &mut self,
        integral: &Integral,
        templ: &GrayF32,
        threshold: f32,
    ) -> Vec<Detection> {
        let (tw, th) = (templ.width() as usize, templ.height() as usize);
        let (iw, ih) = (self.image.width() as usize, self.image.height() as usize);
        if tw > iw || th > ih || tw == 0 || th == 0 {
            return Vec::new();
        }
        let pw = (iw + tw - 1).max(64).next_power_of_two();
        let ph = (ih + th - 1).max(64).next_power_of_two();

        let image = &self.image;
        let t_corr = std::time::Instant::now();
        let group = self
            .groups
            .entry((pw, ph))
            .or_insert_with(|| Group::new(image, pw, ph));
        group.correlate(templ);
        let t_detect = std::time::Instant::now();
        if let Some(p) = &mut self.profile {
            p.calls += 1;
            p.corr_ns += t_corr.elapsed().as_nanos();
        }

        let n = (tw * th) as f64;
        let mut sum_t = 0f64;
        let mut sum_t2 = 0f64;
        for &v in templ.as_raw() {
            sum_t += v as f64;
            sum_t2 += (v as f64) * (v as f64);
        }
        let mean_t = sum_t / n;
        let var_t = sum_t2 / n - mean_t * mean_t;
        if var_t <= 1e-6 {
            return Vec::new();
        }

        let mut out = Vec::new();
        let corr = &group.work;
        for y in 0..=(ih - th) {
            let row = y * pw;
            for x in 0..=(iw - tw) {
                let (sum_i, sum_i2) = integral.window(x, y, tw, th);
                let mean_i = sum_i / n;
                let var_i = sum_i2 / n - mean_i * mean_i;
                if var_i <= 1e-6 {
                    continue;
                }
                let cross = corr[row + x].re as f64;
                let score = ((cross / n - mean_t * mean_i) / (var_t * var_i).sqrt()) as f32;
                if score >= threshold {
                    out.push(Detection {
                        x: x as u32,
                        y: y as u32,
                        w: tw as u32,
                        h: th as u32,
                        score,
                        ink: None,
                    });
                }
            }
        }
        if let Some(p) = &mut self.profile {
            p.detect_ns += t_detect.elapsed().as_nanos();
        }
        out
    }
}
