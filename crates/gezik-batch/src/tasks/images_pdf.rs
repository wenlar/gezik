//! "Images to PDF" as an engine job: the pictures are written into one PDF under a temporary
//! name next to the output (in this process: no pdfium), which is renamed when complete. An
//! existing output goes through the conflict list; one undo trashes the PDF.

use std::io;
use std::path::PathBuf;

use gezik_core::batch::pdf::PageOptions;
use gezik_ops::{Facts, Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};

use super::file_name;
use crate::pdf::images::write_pdf;

/// What Undo says: "Make PDF from 12 pictures", "Make PDF from 1 picture".
pub fn images_pdf_label(count: usize) -> String {
    format!("Make PDF from {}", pictures(count))
}

fn pictures(count: usize) -> String {
    if count == 1 { "1 picture".to_owned() } else { format!("{count} pictures") }
}

pub struct ImagesToPdfTask {
    pictures: Vec<PathBuf>,
    output: PathBuf,
    options: PageOptions,
}

impl ImagesToPdfTask {
    pub fn new(pictures: Vec<PathBuf>, output: PathBuf, options: PageOptions) -> ImagesToPdfTask {
        ImagesToPdfTask { pictures, output, options }
    }
}

impl Task for ImagesToPdfTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Pdf
    }

    fn title(&self) -> String {
        format!("Making {} from {}", file_name(&self.output), pictures(self.pictures.len()))
    }

    fn count(&self) -> usize {
        self.pictures.len()
    }

    fn resources(&self) -> Resources {
        let mut paths = self.pictures.clone();
        paths.push(self.output.clone());
        Resources { paths, work: Work::Cpu }
    }

    fn workers(&self) -> Option<usize> {
        Some(1)
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        // A picture that cannot be read is left out when writing, with its reason.
        let size = self.pictures.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
        let facts = Facts { is_dir: false, size, modified: None };
        sink.item(PlanItem::new(Stage::Parallel, facts).target(&self.output).checked());
    }

    fn run(&self, item: &PlanItem, run: &RunCx<'_>) -> io::Result<Outcome> {
        // The target the conflict list settled on ("Keep both" gives it a number).
        let target = item.target.clone().unwrap_or_else(|| self.output.clone());
        let temp = run.temp_file_for(&target);
        let mut on_picture = |_: &std::path::Path, size: u64| run.add_bytes(size);
        let stop = || run.stopped();
        // `write_pdf` removes its file on every error.
        let made = write_pdf(&self.pictures, &temp, &self.options, &mut on_picture, &stop)?;
        for (path, err) in &made.left_out {
            run.skip(path, err);
        }
        if let Err(err) = gezik_platform::fs::move_entry(&temp, &target) {
            let _ = std::fs::remove_file(&temp);
            return Err(err);
        }
        Ok(Outcome::Created { path: target.clone(), facts: facts_after(&target, false), from: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_and_labels() {
        let task = ImagesToPdfTask::new(vec!["d/a.jpg".into()], "d/a.pdf".into(), PageOptions::DEFAULT);
        assert_eq!(task.title(), "Making a.pdf from 1 picture");
        assert_eq!(task.kind(), TaskKind::Pdf);
        assert_eq!(task.workers(), Some(1));
        assert_eq!(task.resources().work, Work::Cpu);
        assert_eq!(images_pdf_label(12), "Make PDF from 12 pictures");
    }
}
