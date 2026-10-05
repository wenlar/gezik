//! One question at a time over the window: a title, a message and buttons.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::AppWindow;

type Answer = Box<dyn FnOnce(Option<usize>)>;

struct Question {
    title: String,
    message: String,
    buttons: Vec<String>,
    /// The button Esc chooses.
    escape: usize,
    answer: Answer,
}

struct Inner {
    window: slint::Weak<AppWindow>,
    queue: RefCell<VecDeque<Question>>,
    /// The answer of the question on screen.
    open: RefCell<Option<Answer>>,
}

#[derive(Clone)]
pub struct Dialogs(Rc<Inner>);

impl Dialogs {
    pub fn new(window: &AppWindow) -> Dialogs {
        let dialogs =
            Dialogs(Rc::new(Inner { window: window.as_weak(), queue: RefCell::default(), open: RefCell::default() }));
        window.on_dialog_chosen({
            let dialogs = dialogs.clone();
            move |index| dialogs.chosen(index)
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
        self.0.queue.borrow_mut().push_back(Question {
            title: title.into(),
            message: message.into(),
            buttons: buttons.iter().map(|b| (*b).to_owned()).collect(),
            escape,
            answer: Box::new(answer),
        });
        if self.0.open.borrow().is_none() {
            self.show_next();
        }
    }

    fn show_next(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let next = self.0.queue.borrow_mut().pop_front();
        match next {
            Some(question) => {
                window.set_dialog_title(question.title.into());
                window.set_dialog_message(question.message.into());
                window.set_dialog_escape(i32::try_from(question.escape).unwrap_or(0));
                let buttons: Vec<SharedString> = question.buttons.into_iter().map(Into::into).collect();
                window.set_dialog_buttons(ModelRc::new(VecModel::from(buttons)));
                *self.0.open.borrow_mut() = Some(question.answer);
                window.set_dialog_open(true);
            }
            None => {
                window.set_dialog_open(false);
                window.invoke_focus_list();
            }
        }
    }

    fn chosen(&self, index: i32) {
        let answer = self.0.open.borrow_mut().take();
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
