use image::imageops::{self, FilterType};
use image::{Rgb, RgbImage};

const ANALYSIS_MAX_DIMENSION: u32 = 1_024;
const MIN_DOCUMENT_FRACTION: f64 = 0.30;
const MIN_DOCUMENT_FILL: f64 = 0.55;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ScanCleanupOptions {
    pub auto_crop: bool,
    pub correct_perspective: bool,
    pub remove_shadows: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ScanCleanupReport {
    pub page_boundary_detected: bool,
    pub cropped: bool,
    pub perspective_corrected: bool,
    pub shadow_removed: bool,
}

#[derive(Clone, Copy, Debug)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy, Debug)]
struct DocumentGeometry {
    corners: [Point; 4],
    bounds: (u32, u32, u32, u32),
}

pub(crate) fn clean_scan_image<F>(
    image: RgbImage,
    max_width: u32,
    max_height: u32,
    options: ScanCleanupOptions,
    mut checkpoint: F,
) -> Result<(RgbImage, ScanCleanupReport), String>
where
    F: FnMut() -> Result<(), String>,
{
    checkpoint()?;
    let geometry = if options.auto_crop || options.correct_perspective {
        detect_document_geometry(&image)
    } else {
        None
    };
    let mut report = ScanCleanupReport {
        page_boundary_detected: geometry.is_some(),
        ..ScanCleanupReport::default()
    };

    let mut prepared = if options.correct_perspective {
        if let Some(geometry) = geometry {
            let warped = warp_document(
                &image,
                geometry.corners,
                max_width,
                max_height,
                &mut checkpoint,
            )?;
            report.cropped = true;
            report.perspective_corrected = true;
            warped
        } else {
            resize_to_fit(image, max_width, max_height)
        }
    } else if options.auto_crop {
        if let Some(geometry) = geometry {
            let (left, top, right, bottom) = geometry.bounds;
            let cropped = imageops::crop_imm(
                &image,
                left,
                top,
                right.saturating_sub(left).saturating_add(1),
                bottom.saturating_sub(top).saturating_add(1),
            )
            .to_image();
            report.cropped = cropped.width() < image.width() || cropped.height() < image.height();
            resize_to_fit(cropped, max_width, max_height)
        } else {
            resize_to_fit(image, max_width, max_height)
        }
    } else {
        resize_to_fit(image, max_width, max_height)
    };

    checkpoint()?;
    if options.remove_shadows {
        remove_uneven_illumination(&mut prepared, &mut checkpoint)?;
        report.shadow_removed = true;
    }
    checkpoint()?;
    Ok((prepared, report))
}

fn resize_to_fit(image: RgbImage, max_width: u32, max_height: u32) -> RgbImage {
    let (width, height) = fit_dimensions(image.width(), image.height(), max_width, max_height);
    if width == image.width() && height == image.height() {
        image
    } else {
        imageops::resize(&image, width, height, FilterType::Lanczos3)
    }
}

fn fit_dimensions(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let width = width.max(1);
    let height = height.max(1);
    let scale = (f64::from(max_width.max(1)) / f64::from(width))
        .min(f64::from(max_height.max(1)) / f64::from(height))
        .min(1.0);
    (
        (f64::from(width) * scale).round().max(1.0) as u32,
        (f64::from(height) * scale).round().max(1.0) as u32,
    )
}

fn detect_document_geometry(image: &RgbImage) -> Option<DocumentGeometry> {
    if image.width() < 32 || image.height() < 32 {
        return None;
    }
    let (analysis_width, analysis_height) = fit_dimensions(
        image.width(),
        image.height(),
        ANALYSIS_MAX_DIMENSION,
        ANALYSIS_MAX_DIMENSION,
    );
    let analysis = if analysis_width == image.width() && analysis_height == image.height() {
        image.clone()
    } else {
        imageops::resize(image, analysis_width, analysis_height, FilterType::Triangle)
    };
    let width = analysis.width() as usize;
    let height = analysis.height() as usize;
    let band = ((analysis.width().min(analysis.height()) / 40).max(2)) as usize;
    let mut border = Vec::with_capacity((width + height) * band * 2);
    for y in 0..height {
        for x in 0..band.min(width) {
            border.push(*analysis.get_pixel(x as u32, y as u32));
            border.push(*analysis.get_pixel((width - 1 - x) as u32, y as u32));
        }
    }
    for x in band..width.saturating_sub(band) {
        for y in 0..band.min(height) {
            border.push(*analysis.get_pixel(x as u32, y as u32));
            border.push(*analysis.get_pixel(x as u32, (height - 1 - y) as u32));
        }
    }
    if border.is_empty() {
        return None;
    }
    let background = Rgb([
        median_channel(&border, 0),
        median_channel(&border, 1),
        median_channel(&border, 2),
    ]);
    let mut border_distances = border
        .iter()
        .map(|pixel| colour_distance(*pixel, background))
        .collect::<Vec<_>>();
    border_distances.sort_unstable();
    let background_variation =
        border_distances[(border_distances.len().saturating_sub(1) * 9) / 10];
    let threshold = background_variation.saturating_add(16).clamp(24, 112);

    let mut mask = vec![false; width * height];
    let mut row_counts = vec![0_usize; height];
    let mut column_counts = vec![0_usize; width];
    for y in 0..height {
        for x in 0..width {
            if colour_distance(*analysis.get_pixel(x as u32, y as u32), background) >= threshold {
                mask[y * width + x] = true;
                row_counts[y] += 1;
                column_counts[x] += 1;
            }
        }
    }

    let minimum_row = (width as f64 * 0.20).ceil() as usize;
    let minimum_column = (height as f64 * 0.20).ceil() as usize;
    let top = row_counts.iter().position(|count| *count >= minimum_row)?;
    let bottom = row_counts.iter().rposition(|count| *count >= minimum_row)?;
    let left = column_counts
        .iter()
        .position(|count| *count >= minimum_column)?;
    let right = column_counts
        .iter()
        .rposition(|count| *count >= minimum_column)?;
    let candidate_width = right.saturating_sub(left).saturating_add(1);
    let candidate_height = bottom.saturating_sub(top).saturating_add(1);
    if candidate_width as f64 / (width as f64) < MIN_DOCUMENT_FRACTION
        || candidate_height as f64 / (height as f64) < MIN_DOCUMENT_FRACTION
    {
        return None;
    }
    let candidate_area = candidate_width.saturating_mul(candidate_height);
    let foreground = (top..=bottom)
        .map(|y| {
            mask[y * width + left..=y * width + right]
                .iter()
                .filter(|value| **value)
                .count()
        })
        .sum::<usize>();
    if candidate_area == 0 || foreground as f64 / (candidate_area as f64) < MIN_DOCUMENT_FILL {
        return None;
    }
    let horizontal_margin = left + width.saturating_sub(right + 1);
    let vertical_margin = top + height.saturating_sub(bottom + 1);
    if horizontal_margin as f64 / (width as f64) < 0.02
        && vertical_margin as f64 / (height as f64) < 0.02
    {
        return None;
    }

    let mut left_points = Vec::new();
    let mut right_points = Vec::new();
    for y in top..=bottom {
        let row = &mask[y * width..(y + 1) * width];
        let Some(row_left) = row.iter().position(|value| *value) else {
            continue;
        };
        let Some(row_right) = row.iter().rposition(|value| *value) else {
            continue;
        };
        if row_right.saturating_sub(row_left) + 1 >= candidate_width / 3 {
            left_points.push((y as f64, row_left as f64));
            right_points.push((y as f64, row_right as f64));
        }
    }
    let mut top_points = Vec::new();
    let mut bottom_points = Vec::new();
    for x in left..=right {
        let mut column_top = None;
        let mut column_bottom = None;
        for y in top..=bottom {
            if mask[y * width + x] {
                column_top.get_or_insert(y);
                column_bottom = Some(y);
            }
        }
        if let (Some(column_top), Some(column_bottom)) = (column_top, column_bottom) {
            if column_bottom.saturating_sub(column_top) + 1 >= candidate_height / 3 {
                top_points.push((x as f64, column_top as f64));
                bottom_points.push((x as f64, column_bottom as f64));
            }
        }
    }
    let left_line = robust_line_fit(&left_points)?;
    let right_line = robust_line_fit(&right_points)?;
    let top_line = robust_line_fit(&top_points)?;
    let bottom_line = robust_line_fit(&bottom_points)?;
    let mut corners = [
        intersect_lines(left_line, top_line)?,
        intersect_lines(right_line, top_line)?,
        intersect_lines(right_line, bottom_line)?,
        intersect_lines(left_line, bottom_line)?,
    ];
    if !valid_quadrilateral(&corners, analysis.width(), analysis.height()) {
        return None;
    }

    let scale_x = f64::from(image.width().saturating_sub(1))
        / f64::from(analysis.width().saturating_sub(1).max(1));
    let scale_y = f64::from(image.height().saturating_sub(1))
        / f64::from(analysis.height().saturating_sub(1).max(1));
    for corner in &mut corners {
        corner.x = (corner.x * scale_x).clamp(0.0, f64::from(image.width() - 1));
        corner.y = (corner.y * scale_y).clamp(0.0, f64::from(image.height() - 1));
    }
    let minimum_x = corners
        .iter()
        .map(|corner| corner.x)
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.0) as u32;
    let maximum_x = corners
        .iter()
        .map(|corner| corner.x)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(f64::from(image.width() - 1)) as u32;
    let minimum_y = corners
        .iter()
        .map(|corner| corner.y)
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.0) as u32;
    let maximum_y = corners
        .iter()
        .map(|corner| corner.y)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(f64::from(image.height() - 1)) as u32;
    Some(DocumentGeometry {
        corners,
        bounds: (minimum_x, minimum_y, maximum_x, maximum_y),
    })
}

fn median_channel(pixels: &[Rgb<u8>], channel: usize) -> u8 {
    let mut values = pixels
        .iter()
        .map(|pixel| pixel.0[channel])
        .collect::<Vec<_>>();
    values.sort_unstable();
    values[values.len() / 2]
}

fn colour_distance(left: Rgb<u8>, right: Rgb<u8>) -> u8 {
    left.0
        .iter()
        .zip(right.0.iter())
        .map(|(left, right)| left.abs_diff(*right))
        .max()
        .unwrap_or(0)
}

fn robust_line_fit(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    if points.len() < 8 {
        return None;
    }
    let initial = line_fit(points)?;
    let mut residuals = points
        .iter()
        .map(|(independent, dependent)| (dependent - (initial.0 * independent + initial.1)).abs())
        .collect::<Vec<_>>();
    residuals.sort_by(|left, right| left.total_cmp(right));
    let limit = (residuals[residuals.len() / 2] * 3.0).max(2.0);
    let filtered = points
        .iter()
        .copied()
        .filter(|(independent, dependent)| {
            (dependent - (initial.0 * independent + initial.1)).abs() <= limit
        })
        .collect::<Vec<_>>();
    if filtered.len() >= 8 {
        line_fit(&filtered)
    } else {
        Some(initial)
    }
}

fn line_fit(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    let count = points.len() as f64;
    let independent_mean = points.iter().map(|point| point.0).sum::<f64>() / count;
    let dependent_mean = points.iter().map(|point| point.1).sum::<f64>() / count;
    let denominator = points
        .iter()
        .map(|point| (point.0 - independent_mean).powi(2))
        .sum::<f64>();
    if denominator <= f64::EPSILON {
        return None;
    }
    let slope = points
        .iter()
        .map(|point| (point.0 - independent_mean) * (point.1 - dependent_mean))
        .sum::<f64>()
        / denominator;
    Some((slope, dependent_mean - slope * independent_mean))
}

fn intersect_lines(x_from_y: (f64, f64), y_from_x: (f64, f64)) -> Option<Point> {
    let denominator = 1.0 - x_from_y.0 * y_from_x.0;
    if denominator.abs() < 1e-6 {
        return None;
    }
    let x = (x_from_y.0 * y_from_x.1 + x_from_y.1) / denominator;
    let y = y_from_x.0 * x + y_from_x.1;
    (x.is_finite() && y.is_finite()).then_some(Point { x, y })
}

fn valid_quadrilateral(corners: &[Point; 4], width: u32, height: u32) -> bool {
    let tolerance_x = f64::from(width) * 0.08;
    let tolerance_y = f64::from(height) * 0.08;
    if corners.iter().any(|corner| {
        !corner.x.is_finite()
            || !corner.y.is_finite()
            || corner.x < -tolerance_x
            || corner.x > f64::from(width) + tolerance_x
            || corner.y < -tolerance_y
            || corner.y > f64::from(height) + tolerance_y
    }) {
        return false;
    }
    let mut cross_sign = 0.0;
    for index in 0..4 {
        let current = corners[index];
        let next = corners[(index + 1) % 4];
        let after = corners[(index + 2) % 4];
        let cross =
            (next.x - current.x) * (after.y - next.y) - (next.y - current.y) * (after.x - next.x);
        if cross.abs() < 1e-6 {
            return false;
        }
        if index == 0 {
            cross_sign = cross.signum();
        } else if cross.signum() != cross_sign {
            return false;
        }
    }
    let area = polygon_area(corners);
    area >= f64::from(width) * f64::from(height) * 0.20
}

fn polygon_area(corners: &[Point; 4]) -> f64 {
    let mut twice_area = 0.0;
    for index in 0..4 {
        let current = corners[index];
        let next = corners[(index + 1) % 4];
        twice_area += current.x * next.y - current.y * next.x;
    }
    twice_area.abs() / 2.0
}

fn warp_document<F>(
    source: &RgbImage,
    corners: [Point; 4],
    max_width: u32,
    max_height: u32,
    checkpoint: &mut F,
) -> Result<RgbImage, String>
where
    F: FnMut() -> Result<(), String>,
{
    let raw_width = edge_length(corners[0], corners[1])
        .max(edge_length(corners[3], corners[2]))
        .round()
        .max(1.0) as u32;
    let raw_height = edge_length(corners[0], corners[3])
        .max(edge_length(corners[1], corners[2]))
        .round()
        .max(1.0) as u32;
    let (output_width, output_height) =
        fit_dimensions(raw_width, raw_height, max_width, max_height);
    let map = ProjectiveMap::from_unit_square(corners)?;
    let mut output = RgbImage::new(output_width, output_height);
    for y in 0..output_height {
        if y % 32 == 0 {
            checkpoint()?;
        }
        let v = if output_height <= 1 {
            0.0
        } else {
            f64::from(y) / f64::from(output_height - 1)
        };
        for x in 0..output_width {
            let u = if output_width <= 1 {
                0.0
            } else {
                f64::from(x) / f64::from(output_width - 1)
            };
            let point = map.map(u, v);
            output.put_pixel(x, y, bilinear_sample(source, point.x, point.y));
        }
    }
    Ok(output)
}

fn edge_length(left: Point, right: Point) -> f64 {
    (right.x - left.x).hypot(right.y - left.y)
}

struct ProjectiveMap {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
    g: f64,
    h: f64,
}

impl ProjectiveMap {
    fn from_unit_square(corners: [Point; 4]) -> Result<Self, String> {
        let [top_left, top_right, bottom_right, bottom_left] = corners;
        let dx1 = top_right.x - bottom_right.x;
        let dx2 = bottom_left.x - bottom_right.x;
        let dx3 = top_left.x - top_right.x + bottom_right.x - bottom_left.x;
        let dy1 = top_right.y - bottom_right.y;
        let dy2 = bottom_left.y - bottom_right.y;
        let dy3 = top_left.y - top_right.y + bottom_right.y - bottom_left.y;
        let (g, h) = if dx3.abs() < 1e-8 && dy3.abs() < 1e-8 {
            (0.0, 0.0)
        } else {
            let denominator = dx1 * dy2 - dx2 * dy1;
            if denominator.abs() < 1e-8 {
                return Err("The detected page perspective is degenerate.".to_string());
            }
            (
                (dx3 * dy2 - dx2 * dy3) / denominator,
                (dx1 * dy3 - dx3 * dy1) / denominator,
            )
        };
        Ok(Self {
            a: top_right.x - top_left.x + g * top_right.x,
            b: bottom_left.x - top_left.x + h * bottom_left.x,
            c: top_left.x,
            d: top_right.y - top_left.y + g * top_right.y,
            e: bottom_left.y - top_left.y + h * bottom_left.y,
            f: top_left.y,
            g,
            h,
        })
    }

    fn map(&self, u: f64, v: f64) -> Point {
        let denominator = self.g * u + self.h * v + 1.0;
        Point {
            x: (self.a * u + self.b * v + self.c) / denominator,
            y: (self.d * u + self.e * v + self.f) / denominator,
        }
    }
}

fn bilinear_sample(image: &RgbImage, x: f64, y: f64) -> Rgb<u8> {
    let x = x.clamp(0.0, f64::from(image.width().saturating_sub(1)));
    let y = y.clamp(0.0, f64::from(image.height().saturating_sub(1)));
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(image.width() - 1);
    let y1 = (y0 + 1).min(image.height() - 1);
    let tx = x - f64::from(x0);
    let ty = y - f64::from(y0);
    let top_left = image.get_pixel(x0, y0).0;
    let top_right = image.get_pixel(x1, y0).0;
    let bottom_left = image.get_pixel(x0, y1).0;
    let bottom_right = image.get_pixel(x1, y1).0;
    let mut channels = [0_u8; 3];
    for channel in 0..3 {
        let top = f64::from(top_left[channel]) * (1.0 - tx) + f64::from(top_right[channel]) * tx;
        let bottom =
            f64::from(bottom_left[channel]) * (1.0 - tx) + f64::from(bottom_right[channel]) * tx;
        channels[channel] = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8;
    }
    Rgb(channels)
}

fn remove_uneven_illumination<F>(image: &mut RgbImage, checkpoint: &mut F) -> Result<(), String>
where
    F: FnMut() -> Result<(), String>,
{
    let tile_size = (image.width().min(image.height()) / 12).clamp(8, 128);
    let tile_columns = image.width().div_ceil(tile_size);
    let tile_rows = image.height().div_ceil(tile_size);
    let mut background = vec![245_u8; (tile_columns * tile_rows) as usize];
    for tile_y in 0..tile_rows {
        checkpoint()?;
        for tile_x in 0..tile_columns {
            let start_x = tile_x * tile_size;
            let start_y = tile_y * tile_size;
            let end_x = (start_x + tile_size).min(image.width());
            let end_y = (start_y + tile_size).min(image.height());
            let mut histogram = [0_u32; 256];
            for y in start_y..end_y {
                for x in start_x..end_x {
                    histogram[usize::from(luminance(*image.get_pixel(x, y)))] += 1;
                }
            }
            let pixels = (end_x - start_x) * (end_y - start_y);
            let target = pixels.saturating_mul(9).div_ceil(10);
            let mut running = 0_u32;
            let percentile = histogram
                .iter()
                .enumerate()
                .find_map(|(value, count)| {
                    running += count;
                    (running >= target).then_some(value as u8)
                })
                .unwrap_or(245);
            background[(tile_y * tile_columns + tile_x) as usize] = percentile.max(32);
        }
    }

    for y in 0..image.height() {
        if y % 64 == 0 {
            checkpoint()?;
        }
        for x in 0..image.width() {
            let local_background =
                interpolate_background(&background, tile_columns, tile_rows, tile_size, x, y)
                    .max(32.0);
            let factor = (245.0 / local_background).clamp(0.75, 2.5);
            let pixel = image.get_pixel_mut(x, y);
            for channel in &mut pixel.0 {
                *channel = (f64::from(*channel) * factor).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Ok(())
}

fn luminance(pixel: Rgb<u8>) -> u8 {
    ((u32::from(pixel.0[0]) * 299
        + u32::from(pixel.0[1]) * 587
        + u32::from(pixel.0[2]) * 114
        + 500)
        / 1_000) as u8
}

fn interpolate_background(
    background: &[u8],
    columns: u32,
    rows: u32,
    tile_size: u32,
    x: u32,
    y: u32,
) -> f64 {
    let grid_x = (f64::from(x) + 0.5) / f64::from(tile_size) - 0.5;
    let grid_y = (f64::from(y) + 0.5) / f64::from(tile_size) - 0.5;
    let left = grid_x.floor().max(0.0) as u32;
    let top = grid_y.floor().max(0.0) as u32;
    let right = (left + 1).min(columns - 1);
    let bottom = (top + 1).min(rows - 1);
    let tx = (grid_x - f64::from(left)).clamp(0.0, 1.0);
    let ty = (grid_y - f64::from(top)).clamp(0.0, 1.0);
    let value = |column: u32, row: u32| f64::from(background[(row * columns + column) as usize]);
    let upper = value(left, top) * (1.0 - tx) + value(right, top) * tx;
    let lower = value(left, bottom) * (1.0 - tx) + value(right, bottom) * tx;
    upper * (1.0 - ty) + lower * ty
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_a_high_contrast_document_without_upscaling() {
        let mut source = RgbImage::from_pixel(400, 300, Rgb([25, 30, 35]));
        for y in 30..270 {
            for x in 50..350 {
                source.put_pixel(x, y, Rgb([245, 245, 242]));
            }
        }
        let (prepared, report) = clean_scan_image(
            source,
            1_000,
            1_000,
            ScanCleanupOptions {
                auto_crop: true,
                ..ScanCleanupOptions::default()
            },
            || Ok(()),
        )
        .unwrap();

        assert!(report.page_boundary_detected);
        assert!(report.cropped);
        assert!((295..=305).contains(&prepared.width()));
        assert!((235..=245).contains(&prepared.height()));
        assert!(
            prepared
                .get_pixel(prepared.width() / 2, prepared.height() / 2)
                .0[0]
                > 230
        );
    }

    #[test]
    fn leaves_text_on_a_full_white_page_uncropped() {
        let mut source = RgbImage::from_pixel(400, 300, Rgb([250, 250, 250]));
        for y in [80, 120, 160, 200] {
            for row in y..y + 4 {
                for x in 60..340 {
                    source.put_pixel(x, row, Rgb([20, 20, 20]));
                }
            }
        }
        let (prepared, report) = clean_scan_image(
            source,
            1_000,
            1_000,
            ScanCleanupOptions {
                auto_crop: true,
                ..ScanCleanupOptions::default()
            },
            || Ok(()),
        )
        .unwrap();

        assert!(!report.page_boundary_detected);
        assert!(!report.cropped);
        assert_eq!(prepared.dimensions(), (400, 300));
    }

    #[test]
    fn rectifies_a_trapezoid_document() {
        let mut source = RgbImage::from_pixel(500, 400, Rgb([18, 24, 30]));
        let corners = [
            Point { x: 100.0, y: 40.0 },
            Point { x: 420.0, y: 80.0 },
            Point { x: 380.0, y: 350.0 },
            Point { x: 60.0, y: 320.0 },
        ];
        for y in 0..source.height() {
            for x in 0..source.width() {
                if point_in_convex_polygon(
                    Point {
                        x: x as f64,
                        y: y as f64,
                    },
                    &corners,
                ) {
                    source.put_pixel(x, y, Rgb([244, 244, 240]));
                }
            }
        }
        let (prepared, report) = clean_scan_image(
            source,
            1_000,
            1_000,
            ScanCleanupOptions {
                correct_perspective: true,
                ..ScanCleanupOptions::default()
            },
            || Ok(()),
        )
        .unwrap();

        assert!(report.page_boundary_detected);
        assert!(report.perspective_corrected);
        assert!(prepared.width() > 280);
        assert!(prepared.height() > 240);
        assert!(
            prepared
                .get_pixel(prepared.width() / 2, prepared.height() / 2)
                .0[0]
                > 230
        );
    }

    #[test]
    fn shadow_removal_reduces_background_variation() {
        let source = RgbImage::from_fn(240, 160, |x, y| {
            if (74..86).contains(&y) {
                Rgb([18, 18, 18])
            } else {
                let value = 95 + ((x * 130) / 239) as u8;
                Rgb([value, value, value])
            }
        });
        let before_difference =
            source.get_pixel(8, 30).0[0].abs_diff(source.get_pixel(232, 30).0[0]);
        let (prepared, report) = clean_scan_image(
            source,
            1_000,
            1_000,
            ScanCleanupOptions {
                remove_shadows: true,
                ..ScanCleanupOptions::default()
            },
            || Ok(()),
        )
        .unwrap();
        let after_difference =
            prepared.get_pixel(8, 30).0[0].abs_diff(prepared.get_pixel(232, 30).0[0]);

        assert!(report.shadow_removed);
        assert!(after_difference < before_difference / 2);
        assert!(prepared.get_pixel(120, 80).0[0] < 80);
    }

    fn point_in_convex_polygon(point: Point, polygon: &[Point; 4]) -> bool {
        let mut sign = 0.0;
        for index in 0..4 {
            let start = polygon[index];
            let end = polygon[(index + 1) % 4];
            let cross =
                (end.x - start.x) * (point.y - start.y) - (end.y - start.y) * (point.x - start.x);
            if cross.abs() < 1e-6 {
                continue;
            }
            if sign == 0.0 {
                sign = cross.signum();
            } else if cross.signum() != sign {
                return false;
            }
        }
        true
    }
}
