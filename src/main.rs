mod args;
mod watch;

use log::debug;
use watch::Watch;

fn main() {
    // Initialize logging early - sets up the logger from RUST_LOG
    env_logger::init();

    // Parse the command-line arguments
    let args = args::Args::new();

    // If verbose log them
    if args.verbose {
        println!("Args:\n{:?}", args);
    }
    // Also log to the regular logger
    debug!("Args: {}", args);

    // This will block the main thread, using it to process filesystem events until
    // this process is killed.
    Watch::new(args).start();
}
