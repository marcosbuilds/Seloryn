use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

fn main() {
    if env::args().nth(1).as_deref() == Some("--grandchild") {
        fs::write("fence-descendant.pid", std::process::id().to_string()).expect("pid");
        loop {
            thread::sleep(Duration::from_millis(25));
        }
    }

    fs::write("fence-ready", std::process::id().to_string()).expect("ready");
    while !Path::new("fence-go").exists() {
        thread::sleep(Duration::from_millis(5));
    }

    let child = Command::new(env::current_exe().expect("current executable"))
        .arg("--grandchild")
        .spawn()
        .expect("grandchild");
    fs::write("fence-descendant.pid", child.id().to_string()).expect("child pid");
    loop {
        thread::sleep(Duration::from_millis(25));
    }
}
