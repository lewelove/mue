use crate::utils;
use std::path::PathBuf;

pub fn run() {
    if let Some(ctdb) = utils::resolve_ctdbtocid(&PathBuf::from(".")) {
        println!("https://db.cuetools.net/?tocid={ctdb}");
    } else {
        std::process::exit(1);
    }
}
