use crate::args::Args;
use chrono::{DateTime, Local};
use log::{debug, error, info, trace};
use notify::{raw_watcher, Op, Watcher};
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use timer::*;

pub(crate) struct Watch {
    args: Args,
    state: WatchState,
}

impl Watch {
    pub fn new(args: Args) -> Self {
        let state = WatchState {
            timer: timer::Timer::new(),
            guard: None,
            paths: Arc::new(Mutex::new(BTreeSet::new())),
        };
        Self { args, state }
    }

    pub fn start(mut self) {
        info!("Enter watch on {}", self.args.path);
        let (tx, rx) = channel();

        // let mut watcher = watcher(tx, self.args.debounce_delay().to_std().unwrap()).unwrap();
        let mut watcher = raw_watcher(tx).unwrap();
        watcher
            .watch(self.args.dir(), self.args.recursion_mode())
            .expect(
            "Could not create a watcher - no notify support in os? Folder deleted since startup?",
        );

        // Harmless - we really do need it until program exit.
        let a: &'static Args = Box::leak(Box::new(self.args));
        // Need an endless loop here
        let mut loop_ix = 0_usize;
        loop {
            trace!("Loop {}", loop_ix);
            loop_ix += 1;
            match rx.recv() {
                Ok(event) => {
                    debug!("Change: {:?}", event);
                    match event.op {
                        Ok(op) => {
                            // There are a couple of events we don't care about:
                            if !matches!(op, Op::CHMOD | Op::RESCAN) {
                                if let Some(pth) = event.path {
                                    // Test against the -f/--filter regex if there is one
                                    if a.accepts(&pth) {
                                        trace!("Filter regex accepts {:?}", &pth);
                                        self.state = self.state.touch(pth, a);
                                    } else {
                                        debug!("Filter regex REJECTS path {:?}", &pth);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error in watcher: {} for {:?}", e, event.path);
                            if a.exit_on_error {
                                eprintln!("exit-on-error is true - exiting");
                                std::process::exit(10);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{}", e);
                    if a.exit_on_error {
                        eprintln!("exit-on-error is true - exiting");
                        std::process::exit(11);
                    }
                }
            }
        }
    }
}

struct WatchState {
    timer: Timer,
    guard: Option<Guard>,
    paths: Arc<Mutex<BTreeSet<String>>>,
}

impl WatchState {
    fn touch(mut self, path: PathBuf, args: &'static Args) -> Self {
        trace!("Touch path {:?}", path);
        if let Some(s) = path.to_str() {
            let deadline: DateTime<Local> = Local::now() + args.delay();

            let mut set = self.paths.lock().unwrap();
            set.insert(s.to_string());
            drop(set);

            let mux = self.paths.clone();

            trace!("New deadline is {}", deadline);

            let new_guard = self.timer.schedule(deadline, None, move || {
                debug!("Timer tick.");
                emit(&mux, args);
            });

            if let Some(old) = self.guard.replace(new_guard) {
                trace!("Drop old timer guard");
                drop(old)
            } else {
                trace!("No existing timer");
            }
        }
        self
    }
}

fn emit(mux: &Arc<Mutex<BTreeSet<String>>>, args: &Args) {
    let mut set = mux.lock().unwrap();
    let copy = set.clone();
    set.clear();
    drop(set);

    if copy.is_empty() {
        debug!("No changed paths remain in set - already published?");
        return;
    }

    if args.verbose {
        println!("EMIT {:?}", copy);
    }

    debug!("Emit {} changed paths: {:?}", copy.len(), copy);

    let mut v = Vec::with_capacity(copy.len());
    for p in copy {
        if args.relativize_paths {
            let buf = PathBuf::from(p);
            let dir = args.dir();
            v.push(relativize(dir, buf).to_str().unwrap().to_string());
        } else {
            v.push(p);
        }
    }
    args.run_command(&v);
}

fn relativize(base: PathBuf, target: PathBuf) -> PathBuf {
    Path::strip_prefix(target.as_path(), base.as_path())
        .unwrap_or_else(|_| panic!("Path no relative: {:?} and {:?}", base, target))
        .to_path_buf()
}
