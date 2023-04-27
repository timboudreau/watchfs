///! Parses command-line arguments and prints help
use chrono::Duration;
use log::{debug, error, info, trace, warn};
use notify::RecursiveMode;
use regex::Regex;
use std::{
    fmt::Display,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

// Grab some info from Cargo.toml to emit in the help:
const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const NAME: &str = env!("CARGO_PKG_NAME");
const REPO: &str = env!("CARGO_PKG_REPOSITORY");

// Default values
const DEFAULT_DELAY_SECONDS: usize = 30;
const DEFAULT_PATH: &str = "./";

// Arguments
const VERBOSE_SHORT: &str = "-v";
const VERBOSE_LONG: &str = "--verbose";

const HELP_SHORT: &str = "-h";
const HELP_LONG: &str = "--help";

const RELATIVIZE_SHORT: &str = "-r";
const RELATIVIZE_LONG: &str = "--relativize";

const ONCE_SHORT: &str = "-o";
const ONCE_LONG: &str = "--once";

const PASS_CHANGED_PATHS_SHORT: &str = "-p";
const PASS_CHANGED_PATHS_LONG: &str = "--pass-paths";

const SHELL_SHORT: &str = "-l";
const SHELL_LONG: &str = "--shell";

const NON_RECURSIVE_SHORT: &str = "-n";
const NON_RECURSIVE_LONG: &str = "--non-recursive";

const FILTER_SHORT: &str = "-f";
const FILTER_LONG: &str = "--filter";

const SECONDS_SHORT: &str = "-s";
const SECONDS_LONG: &str = "--seconds";

const DIR_SHORT: &str = "-d";
const DIR_LONG: &str = "--dir";

const EXIT_ON_ERROR_SHORT: &str = "-x";
const EXIT_ON_ERROR_LONG: &str = "--exit-on-error";

#[derive(Debug, Clone)]
pub(crate) struct Args {
    /// Whether or not to do some logging straight to stderr
    pub verbose: bool,
    /// Whether to print help to stdout and exit immediately
    help: bool,
    /// The file path - default is the working directory
    pub path: String,
    /// The number of seconds of quiescence needed before we publish/run the command
    pub delay_seconds: usize,
    /// If true, pass the set of changed paths as arguments to the command process
    pub pass_changed_paths: bool,
    /// If true, relative the paths to the value of self.path when passing them to
    /// the command process
    pub relativize_paths: bool,
    /// If true, spawn a shell to run the command in rather than exec'ing it directly
    shell: bool,
    /// If true, exit on any encountered error, including non-zero returns
    pub exit_on_error: bool,
    /// If true, exit after the first successful invocation of the command
    once: bool,
    /// The command and arguments to run
    command: Vec<String>,
    /// If true, don't listen recursively, only listen to files directly in the target folder
    pub non_recursive: bool,
    /// A regex to filter out file changes we don't care about.  It is passed the *fully qualified*
    /// file name
    filter: Option<Regex>,
}

/// Provides reasonable default values
impl Default for Args {
    fn default() -> Args {
        Args {
            verbose: false,
            path: String::from(DEFAULT_PATH),
            help: false,
            delay_seconds: DEFAULT_DELAY_SECONDS,
            pass_changed_paths: true,
            command: vec![],
            relativize_paths: false,
            shell: false,
            exit_on_error: false,
            once: false,
            non_recursive: false,
            filter: None,
        }
    }
}

impl Args {
    #[inline]
    pub fn dir(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }

    #[inline]
    pub fn delay(&self) -> Duration {
        Duration::seconds(self.delay_seconds as i64)
    }

    #[inline]
    pub fn accepts(&self, path: &Path) -> bool {
        if let Some(rex) = &self.filter {
            if let Some(st) = path.to_str() {
                rex.is_match(st)
            } else {
                false
            }
        } else {
            true
        }
    }

    #[inline]
    pub fn recursion_mode(&self) -> RecursiveMode {
        if self.non_recursive {
            RecursiveMode::NonRecursive
        } else {
            RecursiveMode::Recursive
        }
    }

    fn args_as_string(&self, addtl: &Vec<String>) -> String {
        let mut result = String::new();
        for st in &self.command {
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(maybe_quote_or_escape(st).as_str());
        }
        if self.pass_changed_paths {
            for p in addtl {
                if !result.is_empty() {
                    result.push(' ');
                }
                result.push_str(maybe_quote_or_escape(p).as_str());
            }
        }
        result
    }

    pub fn run_command(&self, additional_args: &Vec<String>) {
        let mut cmd: Command = if self.shell {
            // If a shell command, we need to concatenate all of the arguments into a single string
            // and ensure they are escaped
            if cfg!(target_os = "windows") {
                let mut result = Command::new("cmd");
                result.arg("/C");
                result.arg(self.args_as_string(additional_args));
                result
            } else {
                let mut result = Command::new("sh");
                result.arg("-c");
                result.arg(self.args_as_string(additional_args));
                result
            }
        } else {
            let mut result = Command::new(self.command.first().expect("Command is empty"));
            for c in self.command.iter().skip(1) {
                result.arg(c);
            }
            result
        };
        if self.pass_changed_paths && !self.shell {
            // if self.shell, we already appended them above
            for path in additional_args {
                cmd.arg(path);
            }
        }
        info!("Launch {:?}", cmd);
        // Launch the process
        let mut result = cmd.spawn();
        match result.as_mut() {
            Ok(ch) => {
                trace!("Enter wait for {:?}", ch);
                // Wait for the process to exit.  Since we have a single timer thread, this
                // also guarantees we can't be running two copies of the command concurrently
                match ch.wait() {
                    Ok(status) => {
                        // Abort on error if necessary
                        if self.exit_on_error && !status.success() {
                            eprintln!(
                                "Process exited with {} and exit-on-error is set.  Exiting.",
                                status
                            );
                            std::process::exit(12);
                        }
                        if self.verbose {
                            eprintln!("Command success: {:?}", cmd);
                        }
                        if self.once && status.success() {
                            info!("--once was passed and command has succeeded.  Exiting.");
                            std::process::exit(0);
                        }
                    }
                    Err(e) => {
                        if self.verbose {
                            eprintln!("{}", e);
                        }
                        error!("Cmd error: {:?}", e);
                        if self.exit_on_error {
                            error!("Error launching process. Exiting.");
                            std::process::exit(100);
                        }
                    }
                }
            }
            Err(e) => {
                if self.verbose {
                    eprintln!("{}", e);
                }
                error!("Error launching process: {}", e);
                if self.exit_on_error {
                    std::process::exit(101);
                }
            }
        }
    }

    pub fn new() -> Args {
        // Fill in defaults:
        let mut result = Args::default();
        let args: Vec<String> = std::env::args().collect();
        // First argument is the path to this program, so start from 1
        let mut i = 1_usize;

        // Update args with command-line flags
        while i < args.len() {
            if let Some(arg) = args.get(i) {
                trace!("Arg: {}", arg);
                match arg.as_str() {
                    // Simple arguments
                    VERBOSE_SHORT | VERBOSE_LONG => result.verbose = true,
                    HELP_SHORT | HELP_LONG => result.help = true,
                    RELATIVIZE_SHORT | RELATIVIZE_LONG => result.relativize_paths = true,
                    ONCE_SHORT | ONCE_LONG => result.once = true,
                    PASS_CHANGED_PATHS_SHORT | PASS_CHANGED_PATHS_LONG => {
                        result.pass_changed_paths = true
                    }
                    SHELL_SHORT | SHELL_LONG => result.shell = true,
                    NON_RECURSIVE_SHORT | NON_RECURSIVE_LONG => result.non_recursive = true,
                    EXIT_ON_ERROR_SHORT | EXIT_ON_ERROR_LONG => result.exit_on_error = true,
                    FILTER_SHORT | FILTER_LONG => {
                        if let Some(next) = args.get(i + 1) {
                            // Skip looking for a flag in the next one - it's our regex
                            i += 1;
                            match Regex::new(next) {
                                Ok(rex) => result.filter = Some(rex),
                                Err(e) => print_help_and_exit(
                                    9,
                                    Some(format!("Invalid regular expression '{}' - {}", next, e)),
                                ),
                            }
                        } else {
                            print_help_and_exit(
                                8,
                                Some(format!(
                                    "{}/{} must be followed by a regular expression argument",
                                    FILTER_SHORT, FILTER_LONG
                                )),
                            );
                        }
                    }
                    SECONDS_SHORT | SECONDS_LONG => {
                        if let Some(secs) = args.get(i + 1) {
                            // Skip looking for a flag in the next one - it's our value
                            i += 1;
                            match secs.parse() {
                                Ok(seconds) => {
                                    if seconds == 0 {
                                        print_help_and_exit(7, Some("Delay must be > 0"));
                                    }
                                    result.delay_seconds = seconds;
                                }
                                Err(_) => print_help_and_exit(
                                    2,
                                    Some(format!(
                                        "Could not parse {}/{} string '{}' as an integer",
                                        SECONDS_SHORT, SECONDS_LONG, secs
                                    )),
                                ),
                            }
                        } else {
                            print_help_and_exit(
                                3,
                                Some(format!(
                                    "{}/{} must be followed by an integer",
                                    SECONDS_SHORT, SECONDS_LONG
                                )),
                            );
                        }
                    }
                    DIR_SHORT | DIR_LONG => {
                        if let Some(d) = args.get(i + 1) {
                            // Skip looking for a flag in the next one - it's our directory
                            i += 1;
                            let pth = fs::canonicalize(std::path::PathBuf::from(d));
                            match pth {
                                Ok(path) => {
                                    debug!("Target path {} canonicalized to {:?}", d, path);
                                    result.path = path.to_str().unwrap().to_string();
                                }
                                Err(e) => {
                                    error!("Could not canonicalize '{}' : {}", d, e);
                                    print_help_and_exit(
                                        6,
                                        Some(format!(
                                            "Target folder {} cannot be canonicalized: {}",
                                            d, e
                                        )),
                                    );
                                }
                            }
                        } else {
                            info!("Unrecognized argument at {}: '{}' - assume it is start of command to run.", i, arg);
                            print_help_and_exit(
                                5,
                                Some(format!(
                                    "{}/{} must be followed by a file path",
                                    DIR_SHORT, DIR_LONG
                                )),
                            );
                        }
                    }
                    _ => {
                        let mut cmd = Vec::with_capacity(args.len() - i);
                        for j in i..args.len() {
                            cmd.push(args.get(j).expect("missing item").to_string());
                        }
                        result.command = cmd;
                        warn!("No command passed - will use `echo`");
                        break;
                    }
                }
                i += 1;
            } else {
                break;
            }
        }
        if result.help {
            print_help_and_exit::<String>(0, None);
            return result;
        }
        if result.relativize_paths && !result.pass_changed_paths {
            print_help_and_exit(
                4,
                Some(format!(
                    "Can only use {}/{} if -{}/{} is also set.",
                    RELATIVIZE_SHORT,
                    RELATIVIZE_LONG,
                    PASS_CHANGED_PATHS_SHORT,
                    PASS_CHANGED_PATHS_LONG
                )),
            );
        }
        if result.command.is_empty() {
            eprintln!("No command passed - will use `echo`");
            result.pass_changed_paths = true;
            result.shell = true;
            result.command = vec![String::from("echo")];
        }
        if DEFAULT_PATH == result.path.as_str() {
            let pth = fs::canonicalize(result.path.as_str());
            match pth {
                Ok(real_path) => {
                    result.path = real_path
                        .to_str()
                        .expect("Could not convert path to a string")
                        .to_string()
                }
                Err(e) => {
                    print_help_and_exit(
                        6,
                        Some(format!("Working directory . no longer exists? {:?}", e)),
                    );
                }
            }
        }
        result
    }
}

impl Display for Args {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("path: {}, command: {:?}, delay_seconds:{}, non_recursive:{}, pass_changed_paths:{}, relativize_paths:{}, shell:{}, once:{}, exit_on_error:{}, verbose:{}, help:{}, filter:{:?}", 
            self.path,
            self.command,
            self.delay_seconds,
            self.non_recursive,
            self.pass_changed_paths,
            self.relativize_paths,
            self.shell,
            self.once,
            self.exit_on_error,
            self.verbose,
            self.help,
            self.filter,
        ))
    }
}

fn maybe_quote_or_escape(st: &String) -> String {
    if st.contains(' ') || st.contains('\n') || st.contains('\t') {
        let mut result = String::new();
        let quote_char = if st.contains('$') || st.contains('\'') {
            '"'
        } else {
            '\''
        };
        result.push(quote_char);
        result.push_str(st.as_str());
        result.push(quote_char);
        result
    } else {
        // Pending: escape quotes if string conatins ' and or "
        st.to_owned()
    }
}

#[inline]
fn println<A: AsRef<std::ffi::OsStr>>(err: bool, str: A) {
    // Ensure we don't pollute stdout with help content if the context is error-exit
    if err {
        eprintln!("{}", str.as_ref().to_str().unwrap());
    } else {
        println!("{}", str.as_ref().to_str().unwrap());
    }
}

fn print_help(err: bool) {
    println(err, format!("{} {}", NAME, VERSION));

    println(
        err,
        "Generic file-watching with de-bouncing - runs a command on changes once quiescent.\n",
    );
    println(err, format!("Usage: watchfs [{}|{}] [{}|{}] [{}|{} n] [{}|{} regex]\n               [{}|{}] [{}|{}] [{}|{}]\n               [{}|{}] [{}|{}] [{}|{} d] [{}|{}] command args...",
VERBOSE_SHORT, VERBOSE_LONG, HELP_SHORT, HELP_LONG, SECONDS_SHORT, SECONDS_LONG, FILTER_SHORT, FILTER_LONG,
PASS_CHANGED_PATHS_SHORT, PASS_CHANGED_PATHS_LONG, SHELL_SHORT, SHELL_LONG, RELATIVIZE_SHORT, RELATIVIZE_LONG,
ONCE_SHORT, ONCE_LONG, NON_RECURSIVE_SHORT, NON_RECURSIVE_LONG, DIR_SHORT, DIR_LONG, EXIT_ON_ERROR_SHORT, EXIT_ON_ERROR_LONG));

    // println(err, "Usage: watchfs [-v|--verbose] [-h|--help] [-s|--seconds n] [-f|--filter regex]\n              [-p|--pass-changed-paths] [-l|--shell] [-r|--relativize-paths] \n              [-o|--once] [-n|--non-recursive] [-d|dir d] command args...",);
    println(err, "\nWatch a folder for file changes, and run some command after any change,\nonce a timeout has elapsed with no further changes.",);
    println(err, "\nThe trailing portion of the command-line is the command that should be run.  If\nnone is supplied, `echo` will be substituted and paths will be printed to the\nconsole.",);
    println(err, "\nArguments:\n----------");
    println(
        err,
        format!(
            " {} {} d\t\tThe directory to watch (default {})",
            DIR_SHORT, DIR_LONG, DEFAULT_PATH
        ),
    );
    println(err, format!(" {} {} n\t\tThe number of seconds to wait for changes to cease before running the\n\t\t\tcommand (default {})", 
        SECONDS_SHORT, SECONDS_LONG, DEFAULT_DELAY_SECONDS),);
    println(
        err,
        format!(
            " {} {}\t\tExecute the command in a shell (`sh -c` on unix, `cmd /C` on windows)",
            SHELL_SHORT, SHELL_LONG
        ),
    );
    println(
        err,
        format!(
            " {} {}\tPass paths to files that changed as arguments to the command",
            PASS_CHANGED_PATHS_SHORT, PASS_CHANGED_PATHS_LONG
        ),
    );
    println(
        err,
        format!(
            " {} {}\tMake paths to changed files relative to the directory being watched",
            RELATIVIZE_SHORT, RELATIVIZE_LONG
        ),
    );
    println(err, format!(" {} {} regexp\tOnly notify about file paths that match this regular expression\n\t\t\t(matches against the fully qualified path, regardless of -r)[1]",FILTER_SHORT, FILTER_LONG));
    println(
        err,
        format!(
            " {} {}\tExit if the command returns non-zero",
            EXIT_ON_ERROR_SHORT, EXIT_ON_ERROR_LONG
        ),
    );
    println(
        err,
        format!(
            " {} {}\t\tExit after running the command *successfully* (zero exit) once",
            ONCE_SHORT, ONCE_LONG
        ),
    );
    println(err, format!(" {} {} n\tDo not listen to subdirectories of the target directory, only\n\t\t\tthe target.", NON_RECURSIVE_SHORT, NON_RECURSIVE_LONG));
    println(
        err,
        format!(
            " {} {}\t\tDescribe what the application is doing as it does it[2]",
            VERBOSE_SHORT, VERBOSE_LONG
        ),
    );
    println(
        err,
        format!(" {} {}\t\tPrint this help\n", HELP_SHORT, HELP_LONG),
    );
    println(err, "");
    println(
        err,
        " [1] - regex syntax supported by https://docs.rs/regex/latest/regex/",
    );
    println(
        err,
        " [2] - for detailed logging, set this RUST_LOG environment variable to one of info,\n       debug or trace.",
    );

    println(err, "\nThe argument interpreter will assume that all arguments including and subsequent\nto the first argument which is not one of the above starts the command to run on changes.");

    println(err, format!("\nAuthors: {} {}", AUTHORS, REPO));
    // Final trailing newline for formatting
    println(err, "");
}

fn print_help_and_exit<A: AsRef<std::ffi::OsStr>>(code: i32, msg: Option<A>) {
    if let Some(m) = msg {
        println(code != 0, "------------- WatchFS Error -------------");
        println(code != 0, "");
        println(code != 0, m);
        println(code != 0, "");
        println(code != 0, "-----------------------------------------\n");
    }
    print_help(code != 0);
    std::process::exit(code);
}
