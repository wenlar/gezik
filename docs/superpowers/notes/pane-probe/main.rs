// 10a-0 probe (throwaway, not product code): two PaneViews in one window from a
// [PaneData] model, state inside the component, callbacks carry the pane index.
// Modes:
//   check                         scripted events through dispatch_event, PASS/FAIL lines
//   bench <panes> <none|0|both>   idle 4 s, wheel-scroll 5 s, idle 3 s (phases on stdout)
//   openclose                     1 pane, open a second (100k rows), close it again
//   windows                       a second window from the same event loop, then closed
slint::include_modules!();

use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model, ModelNotify, ModelRc, ModelTracker, SharedString, Timer, TimerMode, VecModel};
use std::cell::{Cell, RefCell};
use std::io::Write;
use std::rc::{Rc, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// Rows made on demand, like Gezik's ItemsModel; counts row_data calls.
struct Rows {
    n: usize,
    tag: &'static str,
    calls: Cell<usize>,
    notify: ModelNotify,
}

impl Rows {
    fn new(n: usize, tag: &'static str) -> Rc<Self> {
        Rc::new(Rows { n, tag, calls: Cell::new(0), notify: ModelNotify::default() })
    }
}

impl Model for Rows {
    type Data = Row;
    fn row_count(&self) -> usize {
        self.n
    }
    fn row_data(&self, i: usize) -> Option<Row> {
        if i >= self.n {
            return None;
        }
        self.calls.set(self.calls.get() + 1);
        Some(Row {
            name: format!("{}-file-{i:06}.txt", self.tag).into(),
            size: format!("{} KB", (i * 37) % 9999).into(),
            modified: format!("2026-10-{:02} 12:{:02}", i % 28 + 1, i % 60).into(),
        })
    }
    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

fn pane(rows: &Rc<Rows>, path: &str, active: bool) -> PaneData {
    PaneData { path: path.into(), items: ModelRc::from(rows.clone()), active, ..Default::default() }
}

fn edit(panes: &VecModel<PaneData>, i: usize, f: impl FnOnce(&mut PaneData)) {
    let mut d = panes.row_data(i).unwrap();
    f(&mut d);
    panes.set_row_data(i, d);
}

#[derive(Default)]
struct Log {
    events: RefCell<Vec<String>>,
    scroll: [Cell<f32>; 2],
    focus: [Cell<bool>; 2],
    evals: [Cell<usize>; 2],
}

impl Log {
    fn push(&self, s: String) {
        self.events.borrow_mut().push(s);
    }
    fn has(&self, s: &str) -> bool {
        self.events.borrow().iter().any(|e| e == s)
    }
    fn since(&self, n: usize) -> Vec<String> {
        self.events.borrow()[n..].to_vec()
    }
    fn len(&self) -> usize {
        self.events.borrow().len()
    }
}

fn key_name(t: &str) -> String {
    let k = |k: Key| SharedString::from(k);
    match t {
        _ if t == k(Key::DownArrow).as_str() => "Down".into(),
        _ if t == k(Key::Tab).as_str() => "Tab".into(),
        _ if t == k(Key::Backtab).as_str() => "Backtab".into(),
        _ if t == k(Key::Return).as_str() => "Return".into(),
        _ => t.into(),
    }
}

fn wire(app: &AppWindow, log: &Rc<Log>, panes: &Rc<VecModel<PaneData>>) {
    let (l, p) = (log.clone(), panes.clone());
    app.on_item_pressed(move |pane, row| {
        l.push(format!("press {pane} {row}"));
        for i in 0..p.row_count() {
            edit(&p, i, |d| {
                d.active = i == pane as usize;
                if d.active {
                    d.selected = row;
                }
            });
        }
    });
    let (l, p) = (log.clone(), panes.clone());
    app.on_key(move |pane, text| {
        let name = key_name(&text);
        l.push(format!("key {pane} {name}"));
        if name == "Down" {
            edit(&p, pane as usize, |d| d.selected += 1);
            return true;
        }
        false
    });
    let l = log.clone();
    app.on_window_key(move |e| {
        l.push(format!("wkey {}", key_name(&e.text)));
        false
    });
    let (l, p) = (log.clone(), panes.clone());
    app.on_tab(move |pane, shift| {
        l.push(format!("tab {pane} {shift}"));
        if p.row_count() == 2 {
            let other = 1 - pane as usize;
            for i in 0..2 {
                edit(&p, i, |d| {
                    d.active = i == other;
                    if d.active {
                        d.focus_request += 1;
                    }
                });
            }
        }
    });
    let l = log.clone();
    app.on_scrolled(move |pane, y| l.scroll[pane as usize & 1].set(y));
    let l = log.clone();
    app.on_rename_edited(move |pane, t| l.push(format!("rename-edited {pane} {t}")));
    let l = log.clone();
    app.on_rename_done(move |pane, t| l.push(format!("rename-done {pane} {t}")));
    let l = log.clone();
    app.on_focus_changed(move |pane, has| {
        if pane < 2 {
            l.focus[pane as usize].set(has);
        }
        l.push(format!("focus {pane} {has}"));
    });
    let (l, w) = (log.clone(), app.as_weak());
    app.on_drag_start(move |pane, row| {
        l.push(format!("drag-start {pane} {row}"));
        w.unwrap().set_drag_active(true);
    });
    let w = app.as_weak();
    app.on_drag_move(move |_, x, y| {
        let a = w.unwrap();
        a.set_drag_x(x);
        a.set_drag_y(y);
    });
    let (l, w) = (log.clone(), app.as_weak());
    app.on_drag_end(move |pane, x, y| {
        l.push(format!("drag-end {pane} {x:.0} {y:.0}"));
        w.unwrap().set_drag_active(false);
    });
    let l = log.clone();
    app.on_drop_target(move |pane, row| l.push(format!("drop {pane} {row}")));
    let l = log.clone();
    app.on_ready(move |pane, y| l.push(format!("ready {pane} {:.0}", y.abs())));
    let l = log.clone();
    app.on_geometry(move |pane, x, y, w, h| l.push(format!("geometry {pane} {x:.0} {y:.0} {w:.0} {h:.0}")));
    let l = log.clone();
    app.on_row_eval(move |pane, _| {
        let c = &l.evals[pane as usize & 1];
        c.set(c.get() + 1);
        true
    });
}

fn phase(name: &str) {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    println!("phase {name} {ms}");
    std::io::stdout().flush().ok();
}

type Step = Box<dyn FnOnce()>;

fn chain(mut steps: std::vec::IntoIter<Step>, gap: Duration) {
    if let Some(s) = steps.next() {
        Timer::single_shot(gap, move || {
            s();
            chain(steps, gap);
        });
    }
}

fn at(secs: f32, f: impl FnOnce() + 'static) {
    Timer::single_shot(Duration::from_secs_f32(secs), f);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("check");
    let app = AppWindow::new().unwrap();
    let log = Rc::new(Log::default());
    let panes = Rc::new(VecModel::<PaneData>::default());
    wire(&app, &log, &panes);
    app.set_panes(ModelRc::from(panes.clone()));
    // Kept alive until the loop ends (Timer callbacks and the second window).
    let mut keep: Vec<Box<dyn std::any::Any>> = Vec::new();
    match mode {
        "check" => check(&app, &log, &panes),
        "bench" => {
            let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2);
            let which = args.get(2).cloned().unwrap_or("none".into());
            for i in 0..n {
                panes.push(pane(&Rows::new(100_000, if i == 0 { "a" } else { "b" }), "C:/x", i == 0));
            }
            phase("idle");
            let timer = Rc::new(Timer::default());
            keep.push(Box::new(timer.clone()));
            let w = app.as_weak();
            at(4.0, move || {
                if which == "none" {
                    return;
                }
                phase("scroll");
                let xs: Vec<f32> = match (n, which.as_str()) {
                    (1, _) => vec![500.0],
                    (_, "0") => vec![250.0],
                    _ => vec![250.0, 750.0],
                };
                timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
                    for &x in &xs {
                        w.unwrap().window().dispatch_event(WindowEvent::PointerScrolled {
                            position: LogicalPosition::new(x, 350.0),
                            delta_x: 0.0,
                            delta_y: -40.0,
                        });
                    }
                });
                let t = timer.clone();
                at(5.0, move || {
                    t.stop();
                    phase("after");
                });
            });
            at(12.0, || {
                phase("end");
                slint::quit_event_loop().unwrap();
            });
        }
        "openclose" => {
            panes.push(pane(&Rows::new(100_000, "a"), "C:/a", true));
            phase("one");
            let p = panes.clone();
            at(4.0, move || {
                let rows = Rows::new(100_000, "b");
                let weak = Rc::downgrade(&rows);
                p.push(pane(&rows, "D:/b", false));
                drop(rows);
                phase("two");
                at(4.0, move || {
                    p.remove(1);
                    phase("closed");
                    at(0.5, move || println!("second model freed: {}", weak.upgrade().is_none()));
                });
            });
            at(12.5, || {
                phase("end");
                slint::quit_event_loop().unwrap();
            });
        }
        "windows" => {
            panes.push(pane(&Rows::new(100_000, "a"), "C:/a", true));
            phase("one");
            let second: Rc<RefCell<Option<(AppWindow, Weak<Rows>)>>> = Rc::default();
            keep.push(Box::new(second.clone()));
            let s = second.clone();
            at(4.0, move || {
                phase("create");
                let w2 = AppWindow::new().unwrap();
                let rows = Rows::new(100_000, "w");
                let weak = Rc::downgrade(&rows);
                let model = Rc::new(VecModel::from(vec![pane(&rows, "E:/w", true)]));
                drop(rows);
                w2.set_panes(ModelRc::from(model));
                w2.window().set_position(slint::LogicalPosition::new(120.0, 120.0));
                w2.show().unwrap();
                *s.borrow_mut() = Some((w2, weak));
                phase("two");
                let s = s.clone();
                at(4.0, move || {
                    let (w2, weak) = s.borrow_mut().take().unwrap();
                    w2.hide().unwrap();
                    drop(w2);
                    phase("closed");
                    at(0.5, move || println!("second window model freed: {}", weak.upgrade().is_none()));
                });
            });
            at(12.5, || {
                phase("end");
                slint::quit_event_loop().unwrap();
            });
        }
        _ => panic!("unknown mode {mode}"),
    }
    app.window().set_position(slint::LogicalPosition::new(40.0, 40.0));
    app.show().unwrap();
    slint::run_event_loop_until_quit().unwrap();
    drop(keep);
}

fn check(app: &AppWindow, log: &Rc<Log>, panes: &Rc<VecModel<PaneData>>) {
    let a = Rows::new(100_000, "a");
    let b = Rows::new(100_000, "b");
    panes.push(pane(&a, "C:/a", true));
    panes.push(pane(&b, "D:/b", false));
    let (wa, wb) = (Rc::downgrade(&a), Rc::downgrade(&b));
    drop((a, b));
    let fails = Rc::new(Cell::new(0));
    let ok = {
        let fails = fails.clone();
        move |name: &str, cond: bool, detail: String| {
            if !cond {
                fails.set(fails.get() + 1);
            }
            println!("{} {name}: {detail}", if cond { "PASS" } else { "FAIL" });
        }
    };
    let ok = Rc::new(ok);
    // Counters of row_data and of row binding evaluations, per pane, since the last reset.
    let counts = {
        let (log, wa) = (log.clone(), wa.clone());
        let pane1: Rc<RefCell<Weak<Rows>>> = Rc::new(RefCell::new(wb.clone()));
        let p1 = pane1.clone();
        let read = Rc::new(move |reset: bool| -> String {
            let c = |w: &Weak<Rows>| w.upgrade().map(|r| r.calls.replace(if reset { 0 } else { r.calls.get() })).unwrap_or(0);
            let e = |i: usize| if reset { log.evals[i].replace(0) } else { log.evals[i].get() };
            format!("row_data a={} b={} | row evals p0={} p1={}", c(&wa), c(&p1.borrow()), e(0), e(1))
        });
        (read, pane1)
    };
    let (count, pane1_rows) = counts;
    let w = app.as_weak();
    let key = {
        let w = w.clone();
        move |t: SharedString| {
            let a = w.unwrap();
            a.window().dispatch_event(WindowEvent::KeyPressed { text: t.clone() });
            a.window().dispatch_event(WindowEvent::KeyReleased { text: t });
        }
    };
    let ptr = {
        let w = w.clone();
        move |kind: u8, x: f32, y: f32| {
            let position = LogicalPosition::new(x, y);
            let button = PointerEventButton::Left;
            w.unwrap().window().dispatch_event(match kind {
                0 => WindowEvent::PointerPressed { position, button },
                1 => WindowEvent::PointerMoved { position },
                _ => WindowEvent::PointerReleased { position, button },
            });
        }
    };
    let mark = Rc::new(Cell::new(0usize));
    let mut s: Vec<Step> = Vec::new();
    macro_rules! step {
        ($($c:ident),* => $body:block) => {{ $(let $c = $c.clone();)* s.push(Box::new(move || $body)); }};
    }

    step!(panes => { edit(&panes, 0, |d| d.focus_request += 1); });
    step!(log, ok, count, key, mark => {
        ok("both instances created", log.has("ready 0 0") && log.has("ready 1 0"), format!("{:?}", log.since(0)));
        ok("each pane reports its list rect", log.has("geometry 0 0 24 499 676") && log.has("geometry 1 501 24 499 676"), String::new());
        ok("focus-request focuses pane 0", log.focus[0].get() && !log.focus[1].get(), String::new());
        count(true);
        mark.set(log.len());
        key(Key::DownArrow.into());
    });
    step!(log, ok, count, panes, mark => {
        let ev = log.since(mark.get());
        ok("Down goes to the window capture, then to pane 0 only", ev == ["wkey Down", "key 0 Down"], format!("{ev:?}"));
        ok("selection moved in pane 0", panes.row_data(0).unwrap().selected == 1 && panes.row_data(1).unwrap().selected == 0, String::new());
        println!("INFO after selection change in pane 0: {}", count(true));
        mark.set(log.len());
        edit(&panes, 0, |d| d.path = "C:/a/sub".into());
    });
    step!(log, ok, count, panes, mark => {
        println!("INFO after a scalar (path) change in pane 0: {}", count(true));
        ok("pane 0 keeps focus across set_row_data", log.since(mark.get()).is_empty() && log.focus[0].get(), format!("{:?}", log.since(mark.get())));
        edit(&panes, 1, |d| { d.scroll_to = 240_000.0; d.scroll_request += 1; });
    });
    step!(log, ok, count, panes, pane1_rows => {
        ok("scroll-request scrolls pane 1 only", log.scroll[1].get() == 240_000.0 && log.scroll[0].get() == 0.0,
            format!("p0={} p1={}", log.scroll[0].get(), log.scroll[1].get()));
        println!("INFO after scroll-request in pane 1 (10000 rows down): {}", count(true));
        // Navigation: a new model and the restored scroll in the same PaneData write.
        let n = Rows::new(50_000, "n");
        *pane1_rows.borrow_mut() = Rc::downgrade(&n);
        edit(&panes, 1, |d| { d.items = ModelRc::from(n.clone()); d.path = "D:/b/n".into(); d.scroll_to = 12_000.0; d.scroll_request += 1; });
    });
    step!(log, ok, panes, pane1_rows => {
        ok("new model + scroll-request in one write keeps the scroll", log.scroll[1].get() == 12_000.0, format!("p1={}", log.scroll[1].get()));
        // Two writes in the same tick: model first, then the request.
        let n = Rows::new(50_000, "m");
        *pane1_rows.borrow_mut() = Rc::downgrade(&n);
        edit(&panes, 1, |d| d.items = ModelRc::from(n.clone()));
        edit(&panes, 1, |d| { d.scroll_to = 24_000.0; d.scroll_request += 1; });
    });
    step!(log, ok, panes, pane1_rows => {
        ok("new model, then scroll-request in the same tick", log.scroll[1].get() == 24_000.0, format!("p1={}", log.scroll[1].get()));
        // Gezik's refresh: notify.reset() on the same model, then the request.
        pane1_rows.borrow().upgrade().unwrap().notify.reset();
        edit(&panes, 1, |d| { d.scroll_to = 36_000.0; d.scroll_request += 1; });
    });
    step!(log, ok, key, mark => {
        ok("notify.reset(), then scroll-request in the same tick", log.scroll[1].get() == 36_000.0, format!("p1={}", log.scroll[1].get()));
        mark.set(log.len());
        key(Key::Tab.into());
    });
    step!(log, ok, key, mark => {
        let ev = log.since(mark.get());
        ok("Tab (pane-owned): pane 0 reports it, Rust moves focus to pane 1", log.has("tab 0 false") && log.focus[1].get() && !log.focus[0].get(), format!("{ev:?}"));
        mark.set(log.len());
        key(Key::DownArrow.into());
    });
    step!(log, ok, key, mark => {
        let ev = log.since(mark.get());
        ok("Down now goes to pane 1", ev.contains(&"key 1 Down".to_string()), format!("{ev:?}"));
        mark.set(log.len());
        key(Key::Backtab.into());
    });
    step!(log, ok, key, mark, w => {
        let ev = log.since(mark.get());
        ok("Shift+Tab back to pane 0", log.has("tab 1 true") && log.focus[0].get() && !log.focus[1].get(), format!("{ev:?}"));
        w.unwrap().set_own_tab(false);
        mark.set(log.len());
        key(Key::Tab.into());
    });
    step!(log, key, mark => {
        println!("INFO Slint's own Tab chain from pane 0: {:?} (p0={}, p1={})", log.since(mark.get()), log.focus[0].get(), log.focus[1].get());
        mark.set(log.len());
        key(Key::Tab.into());
    });
    step!(log, panes, mark, w => {
        println!("INFO Slint's own Tab chain, second Tab: {:?} (p0={}, p1={})", log.since(mark.get()), log.focus[0].get(), log.focus[1].get());
        w.unwrap().set_own_tab(true);
        mark.set(log.len());
        edit(&panes, 0, |d| { d.rename_row = 3; d.rename_text = "abc.txt".into(); d.rename_request += 1; });
    });
    step!(log, ok, key, mark => {
        ok("rename-request moves focus into the pane's field", !log.focus[0].get() && !log.focus[1].get(), format!("{:?}", log.since(mark.get())));
        mark.set(log.len());
        key("x".into());
    });
    step!(log, ok, key, mark => {
        let ev = log.since(mark.get());
        ok("typing reaches rename-edited with the pane index", ev.iter().any(|e| e.starts_with("rename-edited 0 ") && e.contains('x')), format!("{ev:?}"));
        mark.set(log.len());
        key(Key::Return.into());
    });
    step!(log, ok, ptr, mark => {
        let ev = log.since(mark.get());
        ok("Return ends the rename in pane 0 and gives focus back", ev.iter().any(|e| e.starts_with("rename-done 0 ")) && log.focus[0].get(), format!("{ev:?}"));
        mark.set(log.len());
        ptr(0, 100.0, 84.0); // pane 0, row 2
    });
    step!(ptr => { ptr(1, 110.0, 84.0); ptr(1, 300.0, 84.0); ptr(1, 750.0, 156.0); }); // into pane 1, 6th visible row
    step!(log, ok, ptr, mark => {
        let ev = log.since(mark.get());
        // Pane 1 is at 36000 px = row 1500, so the 6th visible row is 1505.
        ok("drag from pane 0: pane 1 finds its own drop row from the window drag point",
            ev.contains(&"drag-start 0 2".to_string()) && ev.contains(&"drop 1 1505".to_string()), format!("{ev:?}"));
        mark.set(log.len());
        ptr(2, 750.0, 156.0);
    });
    step!(log, ok, panes, mark => {
        let ev = log.since(mark.get());
        ok("release goes to the pressed pane (0) with window coordinates", ev.iter().any(|e| e == "drag-end 0 750 156"), format!("{ev:?}"));
        panes.remove(1);
    });
    step!(log, ok, mark => {
        let ev = log.since(mark.get());
        ok("closing pane 1 re-reports pane 0's list rect (full width)", ev.iter().any(|e| e == "geometry 0 0 24 1000 676"), format!("{ev:?}"));
    });
    step!(log, ok, panes, pane1_rows, wb => {
        ok("closing pane 1 frees its rows model", pane1_rows.borrow().upgrade().is_none() && wb.upgrade().is_none(), String::new());
        let c = Rows::new(100_000, "c");
        *pane1_rows.borrow_mut() = Rc::downgrade(&c);
        log.scroll[1].set(-1.0);
        let mut d = pane(&c, "E:/c", false);
        d.scroll_to = 4_800.0;
        d.scroll_request = 1;
        panes.push(d);
    });
    // A wheel step in pane 1 reports where its list really is after layout.
    step!(w => { w.unwrap().window().dispatch_event(WindowEvent::PointerScrolled { position: LogicalPosition::new(750.0, 300.0), delta_x: 0.0, delta_y: -24.0 }); });
    step!(log, ok, fails => {
        ok("reopened pane applies its scroll in init (a wheel step then reports 4800 + 24)", log.has("ready 1 4800") && log.scroll[1].get() == 4_824.0,
            format!("ready={:?} scrolled={}", log.events.borrow().iter().filter(|e| e.starts_with("ready 1")).collect::<Vec<_>>(), log.scroll[1].get()));
        println!("RESULT {} failed", fails.get());
        slint::quit_event_loop().unwrap();
    });
    chain(s.into_iter(), Duration::from_millis(300));
}
