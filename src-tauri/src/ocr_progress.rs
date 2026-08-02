const PROGRESS_MARKER: &str = "PAPERWORKS_OCR_PROGRESS_V1";
const MAX_PROGRESS_LINE_BYTES: usize = 16 * 1024;
const MAX_PROGRESS_UNITS: f64 = 1_000_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OcrProgressUpdate {
    pub(crate) percent: u8,
    pub(crate) completed: Option<u32>,
    pub(crate) total: Option<u32>,
}

#[derive(Default)]
pub(crate) struct OcrProgressParser {
    line: Vec<u8>,
    discarding_line: bool,
    last_percent: Option<u8>,
}

impl OcrProgressParser {
    pub(crate) fn push<F>(&mut self, chunk: &[u8], mut on_update: F)
    where
        F: FnMut(OcrProgressUpdate),
    {
        for &byte in chunk {
            if matches!(byte, b'\r' | b'\n') {
                if !self.discarding_line {
                    if let Some(update) = parse_progress_line(&self.line) {
                        if self
                            .last_percent
                            .is_none_or(|percent| update.percent > percent)
                        {
                            self.last_percent = Some(update.percent);
                            on_update(update);
                        }
                    }
                }
                self.line.clear();
                self.discarding_line = false;
                continue;
            }

            if self.discarding_line {
                continue;
            }
            if self.line.len() == MAX_PROGRESS_LINE_BYTES {
                self.line.clear();
                self.discarding_line = true;
                continue;
            }
            self.line.push(byte);
        }
    }
}

pub(crate) fn stage_for_ocr(update: OcrProgressUpdate) -> String {
    match (update.completed, update.total) {
        (Some(completed), Some(total)) => format!(
            "Local OCR: {}% ({} of {} {})",
            update.percent,
            completed,
            total,
            if total == 1 { "page" } else { "pages" }
        ),
        _ => format!("Local OCR: {}%", update.percent),
    }
}

pub(crate) fn is_progress_line(line: &[u8]) -> bool {
    parse_progress_line(line).is_some()
        || String::from_utf8_lossy(line)
            .trim_start()
            .starts_with(PROGRESS_MARKER)
}

fn parse_progress_line(line: &[u8]) -> Option<OcrProgressUpdate> {
    let cleaned = strip_terminal_sequences(line);
    let text = String::from_utf8_lossy(&cleaned);
    let text = text.trim();
    if text.starts_with(PROGRESS_MARKER) {
        return parse_machine_progress(text);
    }
    parse_display_progress(text)
}

fn parse_machine_progress(text: &str) -> Option<OcrProgressUpdate> {
    let mut fields = text.split('\t');
    if fields.next()? != PROGRESS_MARKER {
        return None;
    }
    let percent = parse_percent(fields.next()?)?;
    let completed = parse_bounded_number(fields.next()?)?;
    let total = parse_bounded_number(fields.next()?)?;
    if fields.next().is_some() {
        return None;
    }
    progress_update(percent, completed, total)
}

fn parse_display_progress(text: &str) -> Option<OcrProgressUpdate> {
    let bytes = text.as_bytes();
    if bytes.len() < 4 || !bytes[..3].eq_ignore_ascii_case(b"OCR") {
        return None;
    }
    if !matches!(bytes[3], b':' | b' ' | b'\t') {
        return None;
    }

    let percent_sign = bytes.iter().position(|byte| *byte == b'%')?;
    let percent = parse_number_before(bytes, percent_sign)?;
    let slash = bytes.iter().position(|byte| *byte == b'/')?;
    let completed = parse_number_before(bytes, slash)?;
    let total = parse_number_after(bytes, slash + 1)?;
    progress_update(percent, completed, total)
}

fn progress_update(percent: f64, completed: f64, total: f64) -> Option<OcrProgressUpdate> {
    if !percent.is_finite()
        || !(0.0..=100.0).contains(&percent)
        || !completed.is_finite()
        || !total.is_finite()
        || completed < 0.0
        || total <= 0.0
        || total > MAX_PROGRESS_UNITS
        || completed > total
    {
        return None;
    }
    let calculated_percent = completed * 100.0 / total;
    if (calculated_percent - percent).abs() > 2.0 {
        return None;
    }

    Some(OcrProgressUpdate {
        percent: percent.round().clamp(0.0, 100.0) as u8,
        completed: integral_u32(completed),
        total: integral_u32(total),
    })
}

fn parse_percent(value: &str) -> Option<f64> {
    if value.is_empty() || value.len() > 8 {
        return None;
    }
    parse_bounded_number(value)
}

fn parse_bounded_number(value: &str) -> Option<f64> {
    if value.is_empty() || value.len() > 24 {
        return None;
    }
    let value = value.parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

fn parse_number_before(bytes: &[u8], mut end: usize) -> Option<f64> {
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && is_number_byte(bytes[start - 1]) {
        start -= 1;
    }
    parse_ascii_number(&bytes[start..end])
}

fn parse_number_after(bytes: &[u8], mut start: usize) -> Option<f64> {
    while start < bytes.len() && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    let mut end = start;
    while end < bytes.len() && is_number_byte(bytes[end]) {
        end += 1;
    }
    parse_ascii_number(&bytes[start..end])
}

fn parse_ascii_number(bytes: &[u8]) -> Option<f64> {
    if bytes.is_empty() || bytes.len() > 24 || !bytes.iter().any(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(bytes).ok()?.parse::<f64>().ok()
}

fn is_number_byte(byte: u8) -> bool {
    byte.is_ascii_digit() || matches!(byte, b'.' | b'+' | b'-')
}

fn integral_u32(value: f64) -> Option<u32> {
    let rounded = value.round();
    ((value - rounded).abs() <= 0.001 && (0.0..=u32::MAX as f64).contains(&rounded))
        .then_some(rounded as u32)
}

fn strip_terminal_sequences(line: &[u8]) -> Vec<u8> {
    let mut cleaned = Vec::with_capacity(line.len().min(MAX_PROGRESS_LINE_BYTES));
    let mut index = 0;
    while index < line.len() {
        match line[index] {
            0x1b if line.get(index + 1) == Some(&b'[') => {
                index += 2;
                while index < line.len() {
                    let byte = line[index];
                    index += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        break;
                    }
                }
            }
            0x1b if line.get(index + 1) == Some(&b']') => {
                index += 2;
                while index < line.len() {
                    if line[index] == 0x07 {
                        index += 1;
                        break;
                    }
                    if line[index] == 0x1b && line.get(index + 1) == Some(&b'\\') {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
            }
            0x1b => index = (index + 2).min(line.len()),
            byte if byte == b'\t' || byte >= 0x20 => {
                cleaned.push(byte);
                index += 1;
            }
            _ => index += 1,
        }
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_machine_progress_across_chunks_and_delimiters() {
        let mut parser = OcrProgressParser::default();
        let mut updates = Vec::new();
        parser.push(b"PAPERWORKS_OCR_PROG", |update| updates.push(update));
        parser.push(
            b"RESS_V1\t25\t1\t4\rPAPERWORKS_OCR_PROGRESS_V1\t50\t2\t4\n",
            |update| updates.push(update),
        );

        assert_eq!(
            updates,
            vec![
                OcrProgressUpdate {
                    percent: 25,
                    completed: Some(1),
                    total: Some(4),
                },
                OcrProgressUpdate {
                    percent: 50,
                    completed: Some(2),
                    total: Some(4),
                },
            ]
        );
    }

    #[test]
    fn parses_rich_and_tqdm_ocr_progress() {
        let rich = parse_progress_line(
            b"\x1b[32mOCR                 \xe2\x94\x81\xe2\x94\x81\xe2\x94\x81 42% 5/12 0:00:04\x1b[0m",
        )
        .unwrap();
        let tqdm =
            parse_progress_line(b"OCR: 66.7%|######## | 2.0/3.0 [00:03<00:01, 1.3page/s]").unwrap();

        assert_eq!(rich.percent, 42);
        assert_eq!((rich.completed, rich.total), (Some(5), Some(12)));
        assert_eq!(tqdm.percent, 67);
        assert_eq!((tqdm.completed, tqdm.total), (Some(2), Some(3)));
    }

    #[test]
    fn ignores_unrelated_or_inconsistent_percentages() {
        assert!(parse_progress_line(b"Optimising images: 95% 19/20").is_none());
        assert!(parse_progress_line(b"OCR engine confidence: 95%").is_none());
        assert!(parse_progress_line(b"OCR: 95% 1/20").is_none());
        assert!(parse_progress_line(b"OCR: NaN% 1/2").is_none());
        assert!(parse_progress_line(b"OCR: 50% 3/2").is_none());
        assert!(parse_progress_line(b"PAPERWORKS_OCR_PROGRESS_V1\t50\t1\t2\textra").is_none());
    }

    #[test]
    fn suppresses_duplicate_and_decreasing_updates() {
        let mut parser = OcrProgressParser::default();
        let mut percentages = Vec::new();
        parser.push(
            b"OCR: 50% 1/2\rOCR: 25% 1/4\rOCR: 50% 2/4\rOCR: 75% 3/4\n",
            |update| percentages.push(update.percent),
        );

        assert_eq!(percentages, vec![50, 75]);
    }

    #[test]
    fn discards_an_oversized_line_then_recovers() {
        let mut parser = OcrProgressParser::default();
        let mut updates = Vec::new();
        parser.push(&vec![b'X'; MAX_PROGRESS_LINE_BYTES + 128], |update| {
            updates.push(update)
        });
        assert!(parser.line.is_empty());
        assert!(parser.discarding_line);

        parser.push(b"\nPAPERWORKS_OCR_PROGRESS_V1\t100\t4\t4\n", |update| {
            updates.push(update)
        });
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].percent, 100);
    }

    #[test]
    fn formats_engine_progress_as_a_local_ocr_stage() {
        assert_eq!(
            stage_for_ocr(OcrProgressUpdate {
                percent: 50,
                completed: Some(1),
                total: Some(2),
            }),
            "Local OCR: 50% (1 of 2 pages)"
        );
    }
}
