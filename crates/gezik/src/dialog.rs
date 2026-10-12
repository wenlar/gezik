//! One question at a time over the window: a title, a message, buttons and maybe a text field.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::AppWindow;

type Answer = Box<dyn FnOnce(Option<usize>)>;

/// The line under a field, from its text: `(line, is_error)`.
type Note = Rc<dyn Fn(&str) -> (String, bool)>;
/// Whether a blocking question's field may be answered: the error line to show if not.
type Check = Rc<dyn Fn(&str) -> Result<(), String>>;

struct Question {
    title: String,
    message: String,
    buttons: Vec<String>,
    /// The button Esc chooses.
    escape: usize,
    /// The text field's first text; `None`: no field.
    input: Option<String>,
    /// The field holds a password: dots, with "Show".
    secret: bool,
    /// The engine job asking it: it goes away when the job ends.
    job: Option<u64>,
    /// Writes the line under the field as its text changes; `None`: no line.
    note: Option<Note>,
    /// Check boxes under the field: their labels and marks (`ask_form`).
    checks: Vec<(String, bool)>,
    /// While the line under the field is an error, only the Esc button answers (`ask_form`).
    blocks: bool,
    /// What a blocking question's buttons go by instead of the line (`ask_text_checked`).
    check: Option<Check>,
    answer: Answer,
}

struct Inner {
    window: slint::Weak<AppWindow>,
    queue: RefCell<VecDeque<Question>>,
    /// The answer of the question on screen.
    open: RefCell<Option<Answer>>,
    /// The job asking the question on screen, and its Esc button.
    open_job: Cell<Option<(u64, usize)>>,
    /// The line under the field of the question on screen.
    open_note: RefCell<Option<Note>>,
    /// The question on screen blocks its buttons while its line is an error: its Esc button.
    open_blocks: Cell<Option<usize>>,
    /// The check its buttons go by, if not the line.
    open_check: RefCell<Option<Check>>,
}

#[derive(Clone)]
pub struct Dialogs(Rc<Inner>);

impl Dialogs {
    pub fn new(window: &AppWindow) -> Dialogs {
        let dialogs = Dialogs(Rc::new(Inner {
            window: window.as_weak(),
            queue: RefCell::default(),
            open: RefCell::default(),
            open_job: Cell::new(None),
            open_note: RefCell::new(None),
            open_blocks: Cell::new(None),
            open_check: RefCell::new(None),
        }));
        window.on_dialog_chosen({
            let dialogs = dialogs.clone();
            move |index| dialogs.chosen(index)
        });
        window.on_dialog_edited({
            let dialogs = dialogs.clone();
            move |text| dialogs.edited(&text)
        });
        window.on_dialog_check_toggled({
            let weak = window.as_weak();
            move |index| {
                use slint::Model;
                let (Some(window), Ok(index)) = (weak.upgrade(), usize::try_from(index)) else { return };
                let checks = window.get_dialog_checks();
                if let Some(on) = checks.row_data(index) {
                    checks.set_row_data(index, !on);
                }
            }
        });
        dialogs
    }

    /// Dialogs with no window (tests, a window not attached yet): nothing is ever asked.
    pub fn detached() -> Dialogs {
        Dialogs(Rc::new(Inner {
            window: slint::Weak::default(),
            queue: RefCell::default(),
            open: RefCell::default(),
            open_job: Cell::new(None),
            open_note: RefCell::new(None),
            open_blocks: Cell::new(None),
            open_check: RefCell::new(None),
        }))
    }

    /// Asks; `answer` gets the index of the chosen button (Enter: the first, Esc: the last).
    pub fn ask(
        &self,
        title: impl Into<String>,
        message: impl Into<String>,
        buttons: &[&str],
        answer: impl FnOnce(Option<usize>) + 'static,
    ) {
        self.ask_escape(title, message, buttons, buttons.len().saturating_sub(1), answer);
    }

    /// Like `ask`, but Esc chooses button `escape`.
    pub fn ask_escape(
        &self,
        title: impl Into<String>,
        message: impl Into<String>,
        buttons: &[&str],
        escape: usize,
        answer: impl FnOnce(Option<usize>) + 'static,
    ) {
        self.push(Question {
            title: title.into(),
            message: message.into(),
            buttons: buttons.iter().map(|b| (*b).to_owned()).collect(),
            escape,
            input: None,
            secret: false,
            job: None,
            note: None,
            checks: Vec::new(),
            blocks: false,
            check: None,
            answer: Box::new(answer),
        });
    }

    /// Like `ask_escape`, for engine job `job`: dropped (or closed with Esc) when it ends.
    pub fn ask_for_job(
        &self,
        job: u64,
        title: impl Into<String>,
        message: impl Into<String>,
        buttons: &[&str],
        escape: usize,
        answer: impl FnOnce(Option<usize>) + 'static,
    ) {
        self.push(Question {
            title: title.into(),
            message: message.into(),
            buttons: buttons.iter().map(|b| (*b).to_owned()).collect(),
            escape,
            input: None,
            secret: false,
            job: Some(job),
            note: None,
            checks: Vec::new(),
            blocks: false,
            check: None,
            answer: Box::new(answer),
        });
    }

    /// Engine job `job` ended: its waiting questions go, and the one on screen closes as if
    /// Esc was pressed.
    pub fn forget_job(&self, job: u64) {
        self.0.queue.borrow_mut().retain(|question| question.job != Some(job));
        if let Some((open, escape)) = self.0.open_job.get()
            && open == job
        {
            self.chosen(i32::try_from(escape).unwrap_or(0));
        }
    }

    /// Its window closes (spec 9.3): the question on screen (as it shows now) and those waiting
    /// are asked in `to`, their answers going where they went.
    pub fn hand_over(&self, to: &Dialogs) {
        let open = self.0.open.borrow_mut().take();
        let (job, note) = (self.0.open_job.take(), self.0.open_note.borrow_mut().take());
        let mut moved = std::mem::take(&mut *self.0.queue.borrow_mut());
        if let (Some(answer), Some(window)) = (open, self.0.window.upgrade()) {
            use slint::Model;
            moved.push_front(Question {
                title: window.get_dialog_title().into(),
                message: window.get_dialog_message().into(),
                buttons: window.get_dialog_buttons().iter().map(String::from).collect(),
                escape: usize::try_from(window.get_dialog_escape()).unwrap_or(0),
                input: window.get_dialog_has_input().then(|| window.get_dialog_input().into()),
                secret: window.get_dialog_input_secret(),
                job: job.map(|(job, _)| job),
                note,
                checks: window
                    .get_dialog_check_labels()
                    .iter()
                    .map(String::from)
                    .zip(window.get_dialog_checks().iter())
                    .collect(),
                blocks: self.0.open_blocks.take().is_some(),
                check: self.0.open_check.borrow_mut().take(),
                answer,
            });
        }
        for question in moved {
            to.push(question);
        }
    }

    fn push(&self, question: Question) {
        self.0.queue.borrow_mut().push_back(question);
        if self.0.open.borrow().is_none() {
            self.show_next();
        }
    }

    /// Asks for a text, starting from `initial`; `answer` gets it if the first button is
    /// chosen (Enter), else `None` (Esc: the last button).
    pub fn ask_text(
        &self,
        title: impl Into<String>,
        message: impl Into<String>,
        initial: impl Into<String>,
        buttons: &[&str],
        answer: impl FnOnce(Option<String>) + 'static,
    ) {
        self.ask_input(title.into(), message.into(), (initial.into(), false), buttons, (None, None), Box::new(answer));
    }

    /// Like `ask_text`, with a line under the field that `note` writes as the text changes:
    /// `(line, is_error)`.
    pub fn ask_text_noted(
        &self,
        title: impl Into<String>,
        message: impl Into<String>,
        initial: impl Into<String>,
        buttons: &[&str],
        note: impl Fn(&str) -> (String, bool) + 'static,
        answer: impl FnOnce(Option<String>) + 'static,
    ) {
        let note: Note = Rc::new(note);
        self.ask_input(
            title.into(),
            message.into(),
            (initial.into(), false),
            buttons,
            (None, Some(note)),
            Box::new(answer),
        );
    }

    /// Like `ask_text` with an empty field that shows dots (a password), for engine job `job`
    /// (see `ask_for_job`).
    pub fn ask_password(
        &self,
        job: u64,
        title: impl Into<String>,
        message: impl Into<String>,
        buttons: &[&str],
        answer: impl FnOnce(Option<String>) + 'static,
    ) {
        self.ask_input(
            title.into(),
            message.into(),
            (String::new(), true),
            buttons,
            (Some(job), None),
            Box::new(answer),
        );
    }

    /// Like `ask_text_noted`, but the buttons other than the last (Esc) answer only once `check`
    /// passes; until then they show its error under the field and the question stays open.
    #[allow(clippy::too_many_arguments)]
    pub fn ask_text_checked(
        &self,
        title: impl Into<String>,
        message: impl Into<String>,
        initial: impl Into<String>,
        buttons: &[&str],
        note: impl Fn(&str) -> (String, bool) + 'static,
        check: impl Fn(&str) -> Result<(), String> + 'static,
        answer: impl FnOnce(Option<String>) + 'static,
    ) {
        let window = self.0.window.clone();
        self.push(Question {
            title: title.into(),
            message: message.into(),
            buttons: buttons.iter().map(|b| (*b).to_owned()).collect(),
            escape: buttons.len().saturating_sub(1),
            input: Some(initial.into()),
            secret: false,
            job: None,
            note: Some(Rc::new(note)),
            checks: Vec::new(),
            blocks: true,
            check: Some(Rc::new(check)),
            answer: Box::new(move |choice| {
                answer(window.upgrade().map(|w| w.get_dialog_input().to_string()).filter(|_| choice == Some(0)));
            }),
        });
    }

    /// Like `ask_text_noted`, but every button except the last (Esc) answers, with its index
    /// and the text (Connect / Map to Z: / Cancel).
    pub fn ask_text_choice(
        &self,
        title: impl Into<String>,
        message: impl Into<String>,
        initial: impl Into<String>,
        buttons: &[&str],
        note: impl Fn(&str) -> (String, bool) + 'static,
        answer: impl FnOnce(Option<(usize, String)>) + 'static,
    ) {
        let window = self.0.window.clone();
        let escape = buttons.len().saturating_sub(1);
        self.push(Question {
            title: title.into(),
            message: message.into(),
            buttons: buttons.iter().map(|b| (*b).to_owned()).collect(),
            escape,
            input: Some(initial.into()),
            secret: false,
            job: None,
            note: Some(Rc::new(note)),
            checks: Vec::new(),
            blocks: false,
            check: None,
            answer: Box::new(move |choice| {
                let text = window.upgrade().map(|w| w.get_dialog_input().to_string());
                answer(choice.filter(|&c| c != escape).zip(text));
            }),
        });
    }

    /// Like `ask_text_choice`, with check boxes under the field (`(label, checked)`): every
    /// button except the last (Esc) answers with its index, the text and the marks.
    pub fn ask_form(
        &self,
        title: impl Into<String>,
        message: impl Into<String>,
        (initial, checks): (String, Vec<(String, bool)>),
        buttons: &[&str],
        note: impl Fn(&str) -> (String, bool) + 'static,
        answer: impl FnOnce(Option<(usize, String, Vec<bool>)>) + 'static,
    ) {
        let window = self.0.window.clone();
        let escape = buttons.len().saturating_sub(1);
        self.push(Question {
            title: title.into(),
            message: message.into(),
            buttons: buttons.iter().map(|b| (*b).to_owned()).collect(),
            escape,
            input: Some(initial),
            secret: false,
            job: None,
            note: Some(Rc::new(note)),
            checks,
            blocks: true,
            check: None,
            answer: Box::new(move |choice| {
                use slint::Model;
                let form = window
                    .upgrade()
                    .map(|w| (w.get_dialog_input().to_string(), w.get_dialog_checks().iter().collect::<Vec<bool>>()));
                answer(choice.filter(|&c| c != escape).zip(form).map(|(c, (text, marks))| (c, text, marks)));
            }),
        });
    }

    /// A question with check boxes and no field (`(label, checked)`): every button except the
    /// last (Esc) answers with its index and the marks.
    pub fn ask_checks(
        &self,
        title: impl Into<String>,
        message: impl Into<String>,
        checks: Vec<(String, bool)>,
        buttons: &[&str],
        answer: impl FnOnce(Option<(usize, Vec<bool>)>) + 'static,
    ) {
        let window = self.0.window.clone();
        let escape = buttons.len().saturating_sub(1);
        self.push(Question {
            title: title.into(),
            message: message.into(),
            buttons: buttons.iter().map(|b| (*b).to_owned()).collect(),
            escape,
            input: None,
            secret: false,
            job: None,
            note: None,
            checks,
            blocks: false,
            check: None,
            answer: Box::new(move |choice| {
                use slint::Model;
                let marks = window.upgrade().map(|w| w.get_dialog_checks().iter().collect::<Vec<bool>>());
                answer(choice.filter(|&c| c != escape).zip(marks));
            }),
        });
    }

    /// A password field with no engine job (Connect to Server on Linux); the window keeps no
    /// copy once answered.
    pub fn ask_secret(
        &self,
        title: impl Into<String>,
        message: impl Into<String>,
        buttons: &[&str],
        answer: impl FnOnce(Option<String>) + 'static,
    ) {
        self.ask_input(title.into(), message.into(), (String::new(), true), buttons, (None, None), Box::new(answer));
    }

    fn ask_input(
        &self,
        title: String,
        message: String,
        // The field's first text, and whether it holds a password.
        (initial, secret): (String, bool),
        buttons: &[&str],
        // The engine job asking, and the line under the field.
        (job, note): (Option<u64>, Option<Note>),
        answer: Box<dyn FnOnce(Option<String>)>,
    ) {
        let window = self.0.window.clone();
        self.push(Question {
            title,
            message,
            buttons: buttons.iter().map(|b| (*b).to_owned()).collect(),
            escape: buttons.len().saturating_sub(1),
            input: Some(initial),
            secret,
            job,
            note,
            checks: Vec::new(),
            blocks: false,
            check: None,
            answer: Box::new(move |choice| {
                let text = window.upgrade().map(|w| {
                    let text = w.get_dialog_input().to_string();
                    // A password does not stay in the window.
                    if secret {
                        w.set_dialog_input("".into());
                    }
                    text
                });
                answer(text.filter(|_| choice == Some(0)));
            }),
        });
    }

    fn show_next(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let next = self.0.queue.borrow_mut().pop_front();
        match next {
            Some(question) => {
                window.set_dialog_title(question.title.into());
                window.set_dialog_message(question.message.into());
                window.set_dialog_escape(i32::try_from(question.escape).unwrap_or(0));
                window.set_dialog_has_input(question.input.is_some());
                window.set_dialog_input_secret(question.secret);
                // Each question starts with its text hidden.
                window.set_dialog_show_secret(false);
                let input = question.input.unwrap_or_default();
                let (note, error) = question.note.as_ref().map(|note| note(&input)).unwrap_or_default();
                window.set_dialog_note(note.into());
                window.set_dialog_note_error(error);
                *self.0.open_note.borrow_mut() = question.note;
                window.set_dialog_input(input.into());
                let (labels, marks): (Vec<SharedString>, Vec<bool>) =
                    question.checks.into_iter().map(|(label, on)| (SharedString::from(label), on)).unzip();
                window.set_dialog_check_labels(model_or_none(labels));
                window.set_dialog_checks(model_or_none(marks));
                let buttons: Vec<SharedString> = question.buttons.into_iter().map(Into::into).collect();
                window.set_dialog_buttons(ModelRc::new(VecModel::from(buttons)));
                *self.0.open.borrow_mut() = Some(question.answer);
                self.0.open_job.set(question.job.map(|job| (job, question.escape)));
                self.0.open_blocks.set(question.blocks.then_some(question.escape));
                *self.0.open_check.borrow_mut() = question.check;
                window.set_dialog_open(true);
            }
            None => {
                window.set_dialog_open(false);
                window.set_dialog_check_labels(ModelRc::default());
                window.set_dialog_checks(ModelRc::default());
                window.invoke_focus_list();
            }
        }
    }

    /// Sets the title of the question on screen (a note that follows the field: where to).
    pub fn retitle(&self, title: &str) {
        if let Some(window) = self.0.window.upgrade() {
            window.set_dialog_title(title.into());
        }
    }

    /// The field's text changed: the line under it follows.
    fn edited(&self, text: &str) {
        let Some(note) = self.0.open_note.borrow().clone() else { return };
        let (line, error) = note(text);
        if let Some(window) = self.0.window.upgrade() {
            window.set_dialog_note(line.into());
            window.set_dialog_note_error(error);
        }
    }

    fn chosen(&self, index: i32) {
        if self.refused(index) {
            return;
        }
        self.0.open_blocks.set(None);
        self.0.open_check.borrow_mut().take();
        let answer = self.0.open.borrow_mut().take();
        self.0.open_job.set(None);
        self.0.open_note.borrow_mut().take();
        if let Some(window) = self.0.window.upgrade() {
            window.set_dialog_open(false);
        }
        if let Some(answer) = answer {
            answer(usize::try_from(index).ok());
        }
        // The answer may have asked something new already.
        if self.0.open.borrow().is_none() {
            self.show_next();
        }
    }
}

impl Dialogs {
    /// Whether button `index` stays unanswered: the question blocks, it is not the Esc button and
    /// the line under the field is an error (Save with a name of no known format). The line shows
    /// the error again and the question stays open.
    fn refused(&self, index: i32) -> bool {
        let Some(escape) = self.0.open_blocks.get() else { return false };
        if usize::try_from(index).ok() == Some(escape) {
            return false;
        }
        let Some(window) = self.0.window.upgrade() else { return false };
        let text = window.get_dialog_input();
        let (line, error) = if let Some(check) = self.0.open_check.borrow().clone() {
            check(&text).map_or_else(|why| (why, true), |()| (String::new(), false))
        } else if let Some(note) = self.0.open_note.borrow().clone() {
            note(&text)
        } else {
            return false;
        };
        if error {
            window.set_dialog_note(line.into());
            window.set_dialog_note_error(true);
        }
        error
    }
}

/// `items` as a model; none allocated when there are none (most questions have no check boxes).
fn model_or_none<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    if items.is_empty() { ModelRc::default() } else { ModelRc::new(VecModel::from(items)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_waits_for_a_good_field_and_its_boxes_toggle() {
        crate::windows::tests::platform();
        let window = AppWindow::new().expect("a headless window");
        let dialogs = Dialogs::new(&window);
        let got = Rc::new(RefCell::new(None));
        let answer = {
            let got = got.clone();
            move |a| *got.borrow_mut() = Some(a)
        };
        let note = |t: &str| if t.ends_with(".csv") { (String::new(), false) } else { ("bad".to_owned(), true) };
        let checks = vec![("Only".to_owned(), true), ("Paths".to_owned(), false)];
        dialogs.ask_form("T", "m", ("x.doc".to_owned(), checks), &["Save", "Copy", "Cancel"], note, answer);
        window.invoke_dialog_chosen(0);
        assert!(window.get_dialog_open() && got.borrow().is_none(), "Save is refused while the line is red");
        window.invoke_dialog_chosen(1);
        assert!(got.borrow().is_none(), "Copy too");
        window.invoke_dialog_check_toggled(0);
        window.invoke_dialog_check_toggled(1);
        window.set_dialog_input("x.csv".into());
        window.invoke_dialog_chosen(1);
        assert_eq!(*got.borrow(), Some(Some((1, "x.csv".to_owned(), vec![false, true]))));
        assert!(!window.get_dialog_open());

        // Esc answers whatever the field says; a plain question never blocks.
        let note = |_: &str| ("bad".to_owned(), true);
        let answer = {
            let got = got.clone();
            move |a| *got.borrow_mut() = Some(a)
        };
        dialogs.ask_form("T", "m", ("x".to_owned(), Vec::new()), &["Save", "Cancel"], note, answer);
        window.invoke_dialog_chosen(1);
        assert_eq!(*got.borrow(), Some(None));
        let plain = Rc::new(Cell::new(None));
        let p = plain.clone();
        dialogs.ask_text_noted("T", "m", "x", &["OK", "Cancel"], |_| ("bad".to_owned(), true), move |t| p.set(Some(t)));
        window.invoke_dialog_chosen(0);
        assert_eq!(plain.take(), Some(Some("x".to_owned())), "F5's kind of question keeps its behavior");
    }

    #[test]
    fn a_checked_field_keeps_ok_until_it_is_good() {
        crate::windows::tests::platform();
        let window = AppWindow::new().expect("a headless window");
        let dialogs = Dialogs::new(&window);
        let got = Rc::new(RefCell::new(None));
        let g = got.clone();
        // The line shows a hint, not an error, for a word being typed (`art`): OK goes by the check.
        let note = |_: &str| ("artist, album-artist".to_owned(), false);
        let check = |t: &str| if t == "artist:x" { Ok(()) } else { Err(format!("\"{t}\" is no property")) };
        dialogs.ask_text_checked("T", "m", "art", &["OK", "Cancel"], note, check, move |t| *g.borrow_mut() = Some(t));
        window.invoke_dialog_chosen(0);
        assert!(window.get_dialog_open() && got.borrow().is_none(), "OK is refused");
        assert_eq!(window.get_dialog_note(), "\"art\" is no property");
        assert!(window.get_dialog_note_error());
        window.set_dialog_input("artist:x".into());
        window.invoke_dialog_chosen(0);
        assert_eq!(*got.borrow(), Some(Some("artist:x".to_owned())));
        assert!(!window.get_dialog_open());
    }
}
