//! The preview's PDF pages (spec 13 §5): the client of the long-lived pdfium worker
//! (`gezik --pdf-view`) and the PDF viewer's state. One worker, one thread talking to it, one
//! reading its output. Commands wait in a channel; of those waiting only the newest `render`
//! is sent (older ones are stale). Every reply must come within [`LIMIT`] and pass the
//! protocol's checks, else the worker is ended and the viewer says the PDF could not be shown.

use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use gezik_core::batch::pdf_view::{Command, Reply, fits, read_hello, read_reply};
use gezik_platform::ChildProcess;
use slint::{Rgba8Pixel, SharedPixelBuffer};

use super::zoom::ZoomView;

/// How long a reply may take (a huge scanned page included) before the worker is ended.
pub const LIMIT: Duration = Duration::from_secs(20);
/// How long a worker stays after the preview left PDFs (spec 13 §3.6: the next PDF reuses it).
pub const PARK: Duration = Duration::from_secs(10);

/// What the worker's thread tells the preview.
pub enum Event {
    /// Any reply but a picture.
    Reply(Reply),
    /// A page picture (made ready on the worker's thread); the page's size in points.
    Picture { page_size: (f32, f32), picture: SharedPixelBuffer<Rgba8Pixel> },
    /// The worker failed (crashed, too slow, broke the protocol) and was ended.
    Died(String),
}

/// The running worker. Dropping it closes the channel: its thread ends the process.
pub struct Worker {
    commands: Sender<(u64, Command)>,
    next: u64,
    /// Its thread ended it (set before `Died` is delivered).
    ended: Arc<AtomicBool>,
    /// When the preview left PDFs: it is ended [`PARK`] after.
    pub parked: Option<Instant>,
}

impl Worker {
    /// Starts `program` `args` (Gezik itself with `--pdf-view`); `deliver` gets each reply with
    /// its command's number (on the worker's thread).
    pub fn start(
        program: &Path,
        args: &[&str],
        limit: Duration,
        deliver: impl Fn(u64, Event) + Send + 'static,
    ) -> io::Result<Worker> {
        let (mut process, mut stdin, stdout) = ChildProcess::spawn_binary(program, args)?;
        let (replies_tx, replies) = mpsc::channel();
        std::thread::Builder::new().name("gezik-pdf-read".into()).spawn(move || read_all(stdout, &replies_tx))?;
        let (commands, waiting) = mpsc::channel();
        let ended = Arc::new(AtomicBool::new(false));
        let ended_here = ended.clone();
        std::thread::Builder::new().name("gezik-pdf-view".into()).spawn(move || {
            let ended = talk(&mut stdin, &replies, &waiting, limit, &|id, reply| {
                deliver(
                    id,
                    match reply {
                        Reply::Image { width, height, page_w, page_h, rgba, .. } => Event::Picture {
                            page_size: (page_w, page_h),
                            picture: SharedPixelBuffer::clone_from_slice(&rgba, width, height),
                        },
                        other => Event::Reply(other),
                    },
                )
            });
            drop(stdin);
            process.kill_tree();
            ended_here.store(true, Ordering::SeqCst);
            if let Err(why) = ended {
                let tail = process.stderr_text();
                let tail = tail.trim();
                deliver(0, Event::Died(if tail.is_empty() { why } else { format!("{why}: {tail}") }));
            }
        })?;
        Ok(Worker { commands, next: 0, ended, parked: None })
    }

    /// Whether its process was ended (it failed, or its channel closed).
    pub fn ended(&self) -> bool {
        self.ended.load(Ordering::SeqCst)
    }

    /// Sends `command`; its number (replies carry it), or `None` when the worker is gone.
    pub fn send(&mut self, command: Command) -> Option<u64> {
        self.next += 1;
        self.commands.send((self.next, command)).ok().map(|()| self.next)
    }
}

/// Whether a worker parked at `parked` should be gone by `now`.
pub fn park_over(parked: Option<Instant>, now: Instant) -> bool {
    parked.is_some_and(|at| now.saturating_duration_since(at) >= PARK)
}

/// Reads the greeting, then replies, until the output ends or breaks the protocol.
fn read_all(stdout: impl Read, replies: &Sender<io::Result<Reply>>) {
    let mut input = BufReader::new(stdout);
    if let Err(err) = read_hello(&mut input) {
        let _ = replies.send(Err(err));
        return;
    }
    loop {
        let reply = read_reply(&mut input);
        let end = reply.is_err();
        if replies.send(reply).is_err() || end {
            return;
        }
    }
}

/// The worker thread's loop: sends each command and waits for its reply within `limit`. Ends
/// with `Ok` when the commands' sender goes, with why when the worker fails.
fn talk(
    stdin: &mut dyn Write,
    replies: &Receiver<io::Result<Reply>>,
    commands: &Receiver<(u64, Command)>,
    limit: Duration,
    deliver: &dyn Fn(u64, Reply),
) -> Result<(), String> {
    while let Ok(first) = commands.recv() {
        let mut batch = vec![first];
        batch.extend(commands.try_iter());
        let newest_render = batch.iter().rposition(|(_, c)| matches!(c, Command::Render { .. }));
        for (i, (id, command)) in batch.into_iter().enumerate() {
            let render = matches!(command, Command::Render { .. });
            if render && Some(i) != newest_render {
                continue;
            }
            writeln!(stdin, "{}", command.to_line())
                .and_then(|()| stdin.flush())
                .map_err(|err| format!("cannot write to it ({err})"))?;
            if matches!(command, Command::Close | Command::Quit) {
                continue;
            }
            let reply = match replies.recv_timeout(limit) {
                Ok(Ok(reply)) => reply,
                Ok(Err(err)) => return Err(format!("it broke the protocol ({err})")),
                Err(RecvTimeoutError::Timeout) => return Err("it took too long".into()),
                Err(RecvTimeoutError::Disconnected) => return Err("it stopped".into()),
            };
            if !fits(&reply, &command) {
                return Err("it sent a picture that was not asked for".into());
            }
            deliver(id, reply);
        }
    }
    Ok(())
}

/// Where the PDF viewer is.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    /// pdfium is being looked for.
    Finding,
    Opening,
    /// It is encrypted; `wrong`: the password given did not open it.
    Password {
        wrong: bool,
    },
    Ready,
    /// pdfium is not there; what to install first for downloading to work.
    NoPdfium(Option<String>),
    Failed,
}

/// A page picture: drawn at `scale` (physical pixels per point), `region` of the scaled page.
pub struct Shown {
    pub picture: SharedPixelBuffer<Rgba8Pixel>,
    pub scale: f32,
    pub region: (u32, u32, u32, u32),
}

/// The PDF viewer: the page, the zoom, the picture shown. The worker belongs to the preview.
pub struct PdfView {
    pub path: PathBuf,
    /// From 1.
    pub page: u32,
    /// 0 until it opened.
    pub pages: u32,
    pub zoom: ZoomView,
    /// The page whose size `zoom` has (0: none yet).
    pub sized: u32,
    pub state: State,
    pub shown: Option<Shown>,
    /// The newest command's number: replies to older ones are stale.
    pub wanted: u64,
    /// What the newest `render` asked: scale and region.
    pub asked: (f32, (u32, u32, u32, u32)),
    /// Physical pixels per logical one where it shows.
    pub scale_factor: f32,
}

impl PdfView {
    /// A Letter page until the first picture says the real size; 100% is 96 dpi.
    pub fn new(path: PathBuf, scale_factor: f32, view: (f32, f32)) -> PdfView {
        PdfView {
            path,
            page: 1,
            pages: 0,
            zoom: ZoomView::new((612.0, 792.0), scale_factor * 4.0 / 3.0, view),
            sized: 0,
            state: State::Finding,
            shown: None,
            wanted: 0,
            asked: (0.0, (0, 0, 1, 1)),
            scale_factor,
        }
    }

    /// The next `render`: the visible region at the zoom's scale once the page's size is known,
    /// else the whole page fitted into the view (whose reply says the size).
    pub fn next_render(&self) -> Command {
        if self.sized == self.page {
            let (x, y, w, h) = self.zoom.visible();
            Command::Render { page: self.page, scale: self.zoom.scale(), x, y, w, h }
        } else {
            let (_, _, w, h) = self.zoom.view_region();
            Command::Render { page: self.page, scale: 0.0, x: 0, y: 0, w, h }
        }
    }

    /// Moves `step` pages (the last page's number unknown while opening); whether it moved.
    pub fn step(&mut self, step: i64) -> bool {
        let last = if self.pages == 0 { i64::from(u32::MAX) } else { i64::from(self.pages) };
        let page = (i64::from(self.page) + step).clamp(1, last.max(1)) as u32;
        let moved = page != self.page;
        self.page = page;
        moved
    }

    /// A picture came for the newest render: page size `(w, h)` points, `width` × `height`.
    pub fn take_picture(&mut self, page_size: (f32, f32), picture: SharedPixelBuffer<Rgba8Pixel>) {
        let (scale, (x, y, _, _)) =
            gezik_batch::pdf::view::region(page_size.0, page_size.1, self.asked.0, self.asked.1);
        if self.sized != self.page {
            self.zoom.set_content(page_size);
            self.sized = self.page;
        }
        let region = (x, y, picture.width(), picture.height());
        self.shown = Some(Shown { picture, scale, region });
    }

    /// The bar's note and link for its state.
    pub fn note(&self) -> (&'static str, &'static str) {
        match &self.state {
            State::Finding | State::Opening => ("Opening…", ""),
            State::Password { wrong: false } => ("Password protected", "Enter Password…"),
            State::Password { wrong: true } => ("Wrong password", "Enter Password…"),
            State::NoPdfium(_) => ("More pages need pdfium", "Download"),
            State::Failed => ("This PDF could not be shown", ""),
            State::Ready => ("", ""),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::batch::pdf_view::write_reply;
    use std::io::BufRead;
    use std::sync::{Arc, Mutex};

    /// A worker on in-memory pipes: answers `open` with 3 pages, a render with a picture of
    /// the size asked (or one pixel wider for page 7, or nothing for page 8), counts renders.
    fn fake() -> (impl Write + Send, Receiver<io::Result<Reply>>, Arc<Mutex<Vec<u32>>>) {
        let (to_worker_r, to_worker_w) = std::io::pipe().unwrap();
        let (from_worker_r, mut from_worker_w) = std::io::pipe().unwrap();
        let rendered = Arc::new(Mutex::new(Vec::new()));
        let seen = rendered.clone();
        std::thread::spawn(move || {
            writeln!(from_worker_w, "{}", gezik_core::batch::pdf_view::HELLO).unwrap();
            for line in std::io::BufReader::new(to_worker_r).lines() {
                let reply = match Command::parse(&line.unwrap()).unwrap() {
                    Command::Open { .. } => Reply::Pages(3),
                    Command::Render { page: 8, .. } => {
                        std::thread::sleep(Duration::from_secs(5));
                        continue;
                    }
                    Command::Render { page, w, h, .. } => {
                        seen.lock().unwrap().push(page);
                        let width = if page == 7 { w + 1 } else { w };
                        let rgba = vec![0; (width * h * 4) as usize];
                        Reply::Image { page, width, height: h, page_w: 612.0, page_h: 792.0, rgba }
                    }
                    _ => continue,
                };
                if write_reply(&mut from_worker_w, &reply).is_err() {
                    return;
                }
            }
        });
        let (tx, replies) = mpsc::channel();
        std::thread::spawn(move || read_all(from_worker_r, &tx));
        (to_worker_w, replies, rendered)
    }

    fn render(page: u32) -> Command {
        Command::Render { page, scale: 1.0, x: 0, y: 0, w: 4, h: 2 }
    }

    #[test]
    fn only_the_newest_waiting_render_is_drawn() {
        let (mut stdin, replies, rendered) = fake();
        let (tx, commands) = mpsc::channel();
        let open = Command::Open { library: "l".into(), pdf: "a.pdf".into() };
        for (id, command) in [(1, open), (2, render(1)), (3, render(2)), (4, render(3))] {
            tx.send((id, command)).unwrap();
        }
        drop(tx);
        let got = Mutex::new(Vec::new());
        let ended = talk(&mut stdin, &replies, &commands, LIMIT, &|id, reply| got.lock().unwrap().push((id, reply)));
        assert_eq!(ended, Ok(()));
        let got = got.into_inner().unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], (1, Reply::Pages(3)));
        assert!(matches!(got[1], (4, Reply::Image { page: 3, .. })), "the stale renders 1 and 2 were never sent");
        assert_eq!(*rendered.lock().unwrap(), [3]);
    }

    #[test]
    fn a_picture_not_asked_for_or_a_slow_reply_ends_the_worker() {
        let (mut stdin, replies, _) = fake();
        let (tx, commands) = mpsc::channel();
        tx.send((1, render(7))).unwrap();
        let ended = talk(&mut stdin, &replies, &commands, LIMIT, &|_, _| panic!("nothing is delivered"));
        assert_eq!(ended, Err("it sent a picture that was not asked for".into()));
        let (mut stdin, replies, _) = fake();
        let (tx, commands) = mpsc::channel();
        tx.send((1, render(8))).unwrap();
        let started = Instant::now();
        let ended = talk(&mut stdin, &replies, &commands, Duration::from_millis(200), &|_, _| {});
        assert_eq!(ended, Err("it took too long".into()));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn a_worker_that_does_not_greet_is_a_protocol_failure() {
        let (tx, replies) = mpsc::channel();
        read_all(&b"gezik-pdf 1\n"[..], &tx);
        let (mut stdin, (ctx, commands)) = (Vec::new(), mpsc::channel());
        ctx.send((1, render(1))).unwrap();
        let ended = talk(&mut stdin, &replies, &commands, LIMIT, &|_, _| {});
        assert!(ended.unwrap_err().starts_with("it broke the protocol"));
        // A crash: the output ends.
        let (tx, replies) = mpsc::channel();
        read_all(&b"gezik-pdf-view 1\n"[..], &tx);
        ctx.send((2, render(1))).unwrap();
        assert!(talk(&mut Vec::new(), &replies, &commands, LIMIT, &|_, _| {}).is_err());
    }

    #[test]
    fn a_parked_worker_goes_after_ten_seconds() {
        let at = Instant::now();
        assert!(!park_over(None, at + PARK * 2));
        assert!(!park_over(Some(at), at + Duration::from_secs(9)));
        assert!(park_over(Some(at), at + PARK));
        assert!(!park_over(Some(at + PARK), at), "a clock that went back");
    }

    #[test]
    fn pages_step_within_the_document_and_a_picture_sizes_the_page() {
        let mut pdf = PdfView::new("a.pdf".into(), 1.0, (400.0, 400.0));
        assert!(pdf.step(5), "the count is not known yet");
        pdf.pages = 3;
        pdf.page = 1;
        assert!(!pdf.step(-1) && pdf.step(1) && pdf.step(9));
        assert_eq!(pdf.page, 3);
        let Command::Render { scale, w, h, .. } = pdf.next_render() else { panic!() };
        assert_eq!((scale, w, h), (0.0, 400, 400), "a new page is fitted first");
        pdf.asked = (0.0, (0, 0, 400, 400));
        pdf.take_picture((200.0, 400.0), SharedPixelBuffer::new(200, 400));
        assert_eq!(pdf.sized, 3);
        let shown = pdf.shown.as_ref().unwrap();
        assert_eq!((shown.scale, shown.region), (1.0, (0, 0, 200, 400)));
        let Command::Render { scale, w, h, .. } = pdf.next_render() else { panic!() };
        assert_eq!((scale, w, h), (1.0, 200, 400), "the whole page, now that its size is known");
        assert_eq!(pdf.note(), ("Opening…", ""));
        pdf.state = State::Ready;
        assert_eq!(pdf.note(), ("", ""));
        pdf.state = State::NoPdfium(None);
        assert_eq!(pdf.note(), ("More pages need pdfium", "Download"));
    }
}
