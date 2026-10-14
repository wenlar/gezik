//! Set Dates… (16k): the dialog over the window (set-dates.slint) and its job. One job for
//! all the items: one Ctrl+Z puts every time back.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::SystemTime;

use gezik_core::times::{Fields, Source, parse_local};
use gezik_i18n::t;
use gezik_ops::{JobId, SetTimesTask, Task};
use slint::ComponentHandle;

use crate::AppWindow;
use crate::windows::Slot;

/// Told the job that starts (the Info window reads the items again when it ends).
pub type Then = Box<dyn FnOnce(JobId)>;

struct Open {
    paths: Vec<PathBuf>,
    then: RefCell<Option<Then>>,
}

thread_local! {
    /// Each window's open dialog.
    static OPEN: Slot<Rc<Open>> = const { Slot::new() };
}

/// Opens the dialog for `paths`, its date and time those of the first one's Modified.
pub fn open(paths: Vec<PathBuf>, then: Option<Then>) {
    let Some(first) = paths.first() else { return };
    let shown = gezik_platform::fs::times(first).ok().and_then(|t| t.modified).unwrap_or_else(SystemTime::now);
    let p = gezik_platform::local_date_parts(shown).unwrap_or_default();
    // TRANSLATORS: the Set Dates dialog's title; {} is a file name or "3 items"
    let title = t!("Set Dates of {}", crate::operations::items_text(&paths));
    let id = crate::windows::current();
    OPEN.with(|o| o.set(id, Rc::new(Open { paths, then: RefCell::new(then) })));
    crate::windows::with_window(id, |w| {
        w.set_sd_title(title.into());
        w.set_sd_date(format!("{:04}-{:02}-{:02}", p.year, p.month, p.day).into());
        w.set_sd_time(format!("{:02}:{:02}:{:02}", p.hour, p.minute, p.second).into());
        w.set_sd_modified(true);
        w.set_sd_created(false);
        w.set_sd_accessed(false);
        w.set_sd_has_created(gezik_platform::fs::CAN_SET_CREATED);
        w.set_sd_source(0);
        w.set_sd_from("".into());
        w.set_sd_error("".into());
        w.set_sd_open(true);
    });
}

/// The dialog's buttons, for every window (once).
pub fn connect(window: &AppWindow) {
    let weak = window.as_weak();
    window.on_sd_chosen(move |ok| {
        let Some(w) = weak.upgrade() else { return };
        let Some(open) = OPEN.with(Slot::current) else { return };
        if ok {
            match task(&w, open.paths.clone()) {
                Ok(task) => crate::operations::with_current(|ops| {
                    let id = ops.submit(task, None, crate::operations::After::Nothing);
                    if let Some(then) = open.then.borrow_mut().take() {
                        then(id);
                    }
                }),
                Err(why) => return w.set_sd_error(why.into()),
            }
        }
        w.set_sd_open(false);
        w.invoke_focus_list();
    });
    let weak = window.as_weak();
    window.on_sd_choose_file(move || {
        let Some(w) = weak.upgrade() else { return };
        let Some(base) =
            OPEN.with(Slot::current).and_then(|o| o.paths.first().and_then(|f| f.parent()).map(PathBuf::from))
        else {
            return;
        };
        let initial = w.get_sd_from().to_string();
        let weak = w.as_weak();
        crate::operations::with_current(|ops| {
            ops.dialogs().ask_text(
                t!("From file"),
                t!("File:"),
                initial,
                &[&t!("Choose"), &t!("Cancel")],
                move |text| {
                    let (Some(w), Some(text)) = (weak.upgrade(), text) else { return };
                    let path = base.join(text.trim());
                    if text.trim().is_empty() || gezik_platform::fs::times(&path).is_err() {
                        return w.set_sd_error(t!("No such file: {}", text.trim()).into());
                    }
                    w.set_sd_from(path.display().to_string().into());
                    w.set_sd_source(3);
                    w.set_sd_error("".into());
                },
            );
        });
    });
}

/// The job the dialog's choices make, or what is wrong with them.
fn task(w: &AppWindow, paths: Vec<PathBuf>) -> Result<Box<dyn Task>, String> {
    let fields = Fields {
        modified: w.get_sd_modified(),
        created: w.get_sd_created() && gezik_platform::fs::CAN_SET_CREATED,
        accessed: w.get_sd_accessed(),
    };
    if fields == Fields::default() {
        return Err(t!("Choose at least one date").into_owned());
    }
    let source = match w.get_sd_source() {
        0 => parse_local(&w.get_sd_date(), &w.get_sd_time())
            .and_then(gezik_batch::archive::local_time)
            .map(Source::At)
            .ok_or_else(|| t!("Enter the date as YYYY-MM-DD and the time as HH:MM:SS").into_owned())?,
        1 => Source::At(SystemTime::now()),
        2 => {
            let taken = |path: &std::path::Path| {
                gezik_batch::fields::exif::taken(path).and_then(gezik_batch::archive::local_time)
            };
            return Ok(Box::new(SetTimesTask::taken(paths, fields, Arc::new(taken))));
        }
        _ => {
            let from = PathBuf::from(w.get_sd_from().as_str());
            let times = gezik_platform::fs::times(&from).map_err(|_| t!("Choose a file first").into_owned())?;
            Source::From(times)
        }
    };
    Ok(Box::new(SetTimesTask::new(paths, fields, source)))
}
