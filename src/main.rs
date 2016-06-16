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

extern crate docopt;
extern crate inotify;
extern crate regex;
extern crate rustc_serialize;

use docopt::Docopt;
use inotify::INotify;
use inotify::ffi::*;
use std::collections::HashMap;
use std::env;
use std::io::Write;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;
use std::process::{Command, Stdio};
use regex::Regex;
use regex::RegexSet;


const USAGE: &'static str = "
Usage: dynsync [options] <dest>...

Options:
    --debug                       enable debug
    --rsync=<path>                path to rsync binary
                                  [default: /usr/bin/rsync]
    --root=<directory>            root directory to watch
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
    arg_dest: Vec<String>,
}

fn do_sync(opts: &Options, queue: &mut Vec<PathBuf>) {

    let filelist = queue.iter().map(|x| {
        let mut r = String::from(x.to_str().unwrap());
        r.push('\n');
        r
    }).collect::<String>();

    for dest in &opts.arg_dest {
        let rsync_params = opts.flag_rsync_params
            .split_whitespace()
            .collect::<Vec<_>>();

        let mut cmd = Command::new(opts.flag_rsync.as_str());
        cmd.arg("-a").arg("--relative").arg("--files-from=-")
            .args(&rsync_params)
            .arg(".")
            .arg(dest);
        println!("calling {:?}", cmd);
        let mut process = match cmd.stdin(Stdio::piped()).spawn() {
            Err(why) => {
                println!("couldn't spawn rsync of {}: {}", dest, why);
                continue;
            },
            Ok(process) => process,
        };
        match process.stdin {
            Some(ref mut stdin) => {
                stdin.write_all(filelist.as_bytes()).unwrap()
            },
            None => {
                println!("couldn't send file list to rsync of {}", dest);
                continue;
            }
        }
        match process.wait() {
            Err(why) => {
                println!("rsync of {} exit with error: {}", dest, why);
            },
            Ok(_) => (),
        }
    }
    queue.clear();
    println!("transfer queue is empty now");
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
    let mut ino = INotify::init().unwrap();
    let mut watchlist = HashMap::new();
    let watch_events = IN_MOVED_TO | IN_CLOSE_WRITE;

    // read ignore regular expressions
    let ignore_regex = read_ignore_regex(&opts);

    println!("{:?}", ignore_regex);
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
            let events = ino.available_events().unwrap();

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
                        println!("preparing to synchronizing `{}'. \
                                  current transfer queue length is {}.",
                                 dirname.to_str().unwrap(),
                                 transfer_queue.len());
                    }
                }
            }
        }

        while let Some(dirname) = subdirs.pop() {
            println!("adding new directory `{}' to watchlist",
                     dirname.to_str().unwrap());
            let wd = ino.add_watch(dirname.as_path(), watch_events).unwrap();
            watchlist.insert(wd, dirname);
        }
        if !transfer_queue.is_empty() {
            do_sync(&opts, &mut transfer_queue);
        } else {
            sleep(Duration::from_millis(1000));
        }

    }

}
