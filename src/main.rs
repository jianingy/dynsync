/*

 This piece of code is written by
    Jianing Yang <jianingy.yang@gmail.com>
 with love and passion!

        H A P P Y    H A C K I N G !
              _____               ______
     ____====  ]OO|_n_n__][.      |    |
    [________]_|__|________)<     |YANG|
     oo    oo  'oo OOOO-| oo\\_   ~o~~o~
 +--+--+--+--+--+--+--+--+--+--+--+--+--+
                             14 Jun, 2016

*/

#[macro_use] extern crate log;
extern crate docopt;
extern crate env_logger;
extern crate inotify;
extern crate regex;
extern crate rustc_serialize;
extern crate time;


use docopt::Docopt;
use inotify::INotify;
use inotify::ffi::*;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::io::Write;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;
use std::process::{Command, Stdio};
use regex::RegexSet;


const USAGE: &'static str = "
Usage: dynsync [options] <dest>...
       dynsync (--help | --version)


Options:
    --debug                       enable debug
    --rsync=<path>                path to rsync binary
                                  [default: /usr/bin/rsync]
    --root=<directory>            root directory to watch
    -i, --interval=<time>         check interval (in ms)  [default: 0]
    -x, --ignore-file=<file>      a file of regular expressions of filenames to ignore
    -P, --rsync-params=<params>   rsync parameters separated by whitespaces
    -h, --help                    show this message
    -v, --version                 show version

Bug reports to jianingy.yang@gmail.com
";

#[derive(Debug, RustcDecodable)]
struct Options {
    flag_debug: bool,
    flag_root: String,
    flag_rsync: String,
    flag_rsync_params: String,
    flag_ignore_file: String,
    flag_interval: u64,
    arg_dest: Vec<String>,
}

fn do_sync(opts: &Options, queue: &mut Vec<PathBuf>) -> Result<(), Box<Error>> {

    let filelist = queue.iter()
        .map(|x| x.to_str())
        .filter(|x| x.is_some())
        .filter(|x| x.is_some())
        .map(|x| x.unwrap())
        .collect::<Vec<_>>()
        .join("\n");

    for dest in &opts.arg_dest {
        let rsync_params = opts.flag_rsync_params
            .split_whitespace()
            .collect::<Vec<_>>();

        let mut cmd = Command::new(opts.flag_rsync.as_str());
        cmd.arg("-a").arg("--relative").arg("--files-from=-")
            .args(&rsync_params)
            .arg(".")
            .arg(dest);
        debug!("calling {:?}", cmd);
        let rsync_started_at = time::now();
        let mut process = match cmd.stdin(Stdio::piped()).spawn() {
            Err(why) => {
                warn!("couldn't spawn rsync of {}: {}", dest, why);
                continue;
            },
            Ok(process) => process,
        };
        match process.stdin {
            Some(ref mut stdin) => {
                try!(stdin.write_all(filelist.as_bytes()))
            },
            None => {
                warn!("couldn't send file list to rsync of {}", dest);
                continue;
            }
        }
        match process.wait() {
            Err(why) => {
                warn!("rsync of {} exit with error: {}", dest, why);
            },
            Ok(_) => (),
        }
        let rsync_duration = time::now() - rsync_started_at;
        info!("rsync of {} finished in {} ms",
              dest, rsync_duration.num_milliseconds())
    }
    queue.clear();
    info!("transfer queue is empty now");
    Ok(())
}

fn read_ignore_regex(opts: &Options) -> Option<RegexSet> {
    if opts.flag_ignore_file.is_empty() {
        return None;
    }
    let mut file = File::open(
        Path::new(opts.flag_ignore_file.as_str())).unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    Some(RegexSet::new(content.lines()).unwrap())
}

fn main() {
    let opts: Options = Docopt::new(USAGE)
        .and_then(|d| d.decode())
        .unwrap_or_else(|e| e.exit());

    // initialize logger
    env_logger::init().unwrap();

    let mut ino = INotify::init().unwrap();
    let mut watchlist = HashMap::new();
    let watch_events = IN_CREATE | IN_MOVED_TO | IN_CLOSE_WRITE;

    // read ignore regular expressions
    let ignore_regex = read_ignore_regex(&opts);

    // change working directory
    env::set_current_dir(Path::new(opts.flag_root.as_str())).unwrap();

    // add root to watch list
    let root = PathBuf::from(".");
    let root_wd = ino.add_watch(root.as_path(), watch_events).unwrap();
    watchlist.insert(root_wd, root.clone());


    // add subdirectories under root
    let mut ready = vec![root.clone()];

    while !ready.is_empty() {
        let mut working = ready.clone();
        ready.clear();
        while let Some(cwd) = working.pop() {
            for entry in fs::read_dir(cwd.as_path()).unwrap() {
                let entry = entry.unwrap();
                if fs::metadata(entry.path()).unwrap().is_dir() {
                    ready.push(entry.path());
                }
            }
            let wd = ino.add_watch(cwd.as_path(), watch_events).unwrap();
            watchlist.insert(wd, cwd.clone());
        }
    }

    // wait for events and processing
    let mut transfer_queue: Vec<PathBuf> = Vec::new();
    // let mut waiting_queue: Vec<PathBuf> = Vec::new();
    loop {
        let mut subdirs: Vec<PathBuf> = Vec::new();

        {
            // check if there is any file changed.
            let events = ino.wait_for_events().unwrap();

            for event in events.iter() {
                if let Some(rex) = ignore_regex.clone() {
                    if rex.is_match(event.name.as_str()) {
                        continue;
                    }
                }
                if event.is_dir() {
                    if event.is_create() {
                        // when a new directory created, add it to
                        // the watchlist.
                        let mut dirname = watchlist.get(&event.wd)
                            .unwrap().clone();
                        dirname.push(&event.name);
                        subdirs.push(dirname);
                    }
                } else {
                    if event.is_close_write() || event.is_moved_to() {
                        let mut dirname = watchlist.get(&event.wd)
                            .unwrap().clone();
                        dirname.push(&event.name);
                        transfer_queue.push(dirname.clone());
                        info!("preparing to synchronizing `{}'. \
                               current transfer queue length is {}.",
                              dirname.to_str().unwrap(),
                              transfer_queue.len());
                    }
                }
            }
        }

        while let Some(dirname) = subdirs.pop() {
            info!("adding new directory `{}' to watchlist",
                  dirname.to_str().unwrap());
            let wd = ino.add_watch(dirname.as_path(), watch_events).unwrap();
            watchlist.insert(wd, dirname);
        }
        if !transfer_queue.is_empty() {
            do_sync(&opts, &mut transfer_queue).unwrap();
        } else {
            sleep(Duration::from_millis(opts.flag_interval));
        }

    }

}
