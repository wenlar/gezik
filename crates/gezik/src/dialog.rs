//! One question at a time over the window: a title, a message, buttons and maybe a text field.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::AppWindow;

type Answer = Box<dyn FnOnce(Option<usize>)>;

/// The line under a field, from its text: `(line, is_error)`.
type Note = Rc<dyn Fn(&str) -> (String, bool)>;

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
        }));
        window.on_dialog_chosen({
            let dialogs = dialogs.clone();
            move |index| dialogs.chosen(index)
        });
        window.on_dialog_edited({
            let dialogs = dialogs.clone();
            move |text| dialogs.edited(&text)
        });
        dialogs
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
                let buttons: Vec<SharedString> = question.buttons.into_iter().map(Into::into).collect();
                window.set_dialog_buttons(ModelRc::new(VecModel::from(buttons)));
                *self.0.open.borrow_mut() = Some(question.answer);
                self.0.open_job.set(question.job.map(|job| (job, question.escape)));
                window.set_dialog_open(true);
            }
            None => {
                window.set_dialog_open(false);
                window.invoke_focus_list();
            }
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
