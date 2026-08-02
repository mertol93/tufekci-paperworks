use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub(crate) const PDF_JOB_CANCELLED_ERROR: &str = "The PDF job was cancelled.";

#[derive(Clone)]
pub(crate) struct PdfJobExecutionControl {
    cancelled: Arc<AtomicBool>,
    progress: Arc<dyn Fn(u8, String) + Send + Sync>,
}

impl PdfJobExecutionControl {
    pub(crate) fn direct() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(|_, _| {}),
        }
    }

    pub(crate) fn new(
        cancelled: Arc<AtomicBool>,
        progress: Arc<dyn Fn(u8, String) + Send + Sync>,
    ) -> Self {
        Self {
            cancelled,
            progress,
        }
    }

    pub(crate) fn checkpoint(&self, progress: u8, stage: impl Into<String>) -> Result<(), String> {
        self.ensure_not_cancelled()?;
        (self.progress)(progress.min(100), stage.into());
        self.ensure_not_cancelled()
    }

    pub(crate) fn ensure_not_cancelled(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err(PDF_JOB_CANCELLED_ERROR.to_string())
        } else {
            Ok(())
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn subrange(&self, start: u8, end: u8, prefix: String) -> Self {
        let start = start.min(99);
        let end = end.clamp(start, 99);
        let span = end.saturating_sub(start);
        let parent_progress = Arc::clone(&self.progress);
        Self {
            cancelled: Arc::clone(&self.cancelled),
            progress: Arc::new(move |progress, stage| {
                let mapped = start
                    .saturating_add(((u16::from(span) * u16::from(progress.min(100))) / 100) as u8);
                let stage = if prefix.is_empty() {
                    stage
                } else {
                    format!("{prefix}: {stage}")
                };
                parent_progress(mapped, stage);
            }),
        }
    }
}
