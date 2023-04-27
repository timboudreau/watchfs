WatchFS
=======

A straightforward cli utility to watch a folder for file change notifications, and
then run some command once file changes have been quiescent for a settable number of
seconds.

This can be used to, say, initiate `rsync` when something under a folder changes,
with best-effort avoidance of invoking that command while files are being updated
(think: a compilation job generating a flurry of changes - you don't want to react
to each one, you want to react when it's *done*).

I've seen this pattern many times in my career, and written it into various
applications for various reasons at different times *because there was no generic
utility* that would watch a folder and run **whatever** once things had settled down -
for example, when I was at Amazon, there was a commonly used tool for sync'ing
sources to a server you could test on, which used exactly this pattern.

The current help:

```
watchfs 0.1.0
Generic file-watching with de-bouncing - runs a command on changes once quiescent.

Usage: watchfs [-v|--verbose] [-h|--help] [-s|--seconds n] [-f|--filter regex]
               [-p|--pass-paths] [-l|--shell] [-r|--relativize]
               [-o|--once] [-n|--non-recursive] [-d|--dir d] [-x|--exit-on-error] command args...

Watch a folder for file changes, and run some command after any change,
once a timeout has elapsed with no further changes.

The trailing portion of the command-line is the command that should be run.  If
none is supplied, `echo` will be substituted and paths will be printed to the
console.

Arguments:
----------
 -d --dir d		The directory to watch (default ./)
 -s --seconds n		The number of seconds to wait for changes to cease before running the
			command (default 30)
 -l --shell		Execute the command in a shell (`sh -c` on unix, `cmd /C` on windows)
 -p --pass-paths	Pass paths to files that changed as arguments to the command
 -r --relativize	Make paths to changed files relative to the directory being watched
 -f --filter regexp	Only notify about file paths that match this regular expression
			(matches against the fully qualified path, regardless of -r)[1]
 -x --exit-on-error	Exit if the command returns non-zero
 -o --once		Exit after running the command *successfully* (zero exit) once
 -n --non-recursive n	Do not listen to subdirectories of the target directory, only
			the target.
 -v --verbose		Describe what the application is doing as it does it[2]
 -h --help		Print this help


 [1] - regex syntax supported by https://docs.rs/regex/latest/regex/
 [2] - for detailed logging, set this RUST_LOG environment variable to one of info,
       debug or trace.

The argument interpreter will assume that all arguments including and subsequent
to the first argument which is not one of the above starts the command to run on changes.

Authors: Tim Boudreau <tim@timboudreau.com> 
```

Races
-----

WatchFS does not protect against races in any way other than delaying running
the passed program until some number of seconds have passed without another change.
It is entirely possible for a write to be initiated *after* the process to run on
changes starts and *before* it has exited.  If seeing half-written data (depending
on the atomicity of your filesystem and how the thing making changes does its writes)
is *catastrophic* then you need to do some sort of file-locking.  *That is a contract
between the thing doing the writing and the program that gets run after changes*.


Shell Quoting
-------------

Basic support for shell quoting is present - if running with `-l/--shell` and an argument
contains spaces, it will be single-quoted; if it contains a '$' it will be double-quoted.

Complex escaping of strings containing, say, both `'` and `"` is not currently handled
(it will vary by shell and OS and is rather a can of worms).


Logging / Debugging
-------------------

This project uses the Rust [`env_logger`](https://docs.rs/env_logger/latest/env_logger/) crate
for logging, and can show all of the gory details of filesystem notifications, commands run,
exit codes and event delivery by setting the `RUST_LOG` environment variable.

`RUST_LOG=trace` level is very noisy but show everything.  `debug` level is usually sufficent
to monitor filesystem events directly.  `info` will show high-level events only.


Exit Codes
----------

The following exit codes have specific meanings which may be useful in scripts:

* 2 - unparseable seconds value for -s
* 3 - -s is last argument and no seconds value provided
* 4 - -r passed but -p is unset
* 5 - -d is last argument and no folder follows it
* 6 - target folder does not exist or cannot be resolved
* 7 - delay is 0
* 8 - missing regex for -f
* 9 - invalid regex for -f
* 10 - error received by file watcher and -x is set
* 11 - error fetching events from file watcher and -x is set
* 12 - command exited non-zero and -x is set


Cross Platform Capability
-------------------------

Tested on Linux; Windows theoretically should work but I have no way to test that.
