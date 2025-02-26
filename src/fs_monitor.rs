use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Duration;

use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};
use threadpool::ThreadPool;

use crate::engine::Engine;
use crate::Arguments;

pub(crate) fn start(args: Arguments, engine: Engine) -> Result<(), String> {
    // create a recursive filesystem monitor for the root path
    log::info!("initializing filesystem monitor for '{}' ...", &args.root);

    let (tx, rx) = channel();
    let mut watcher = watcher(tx, Duration::ZERO).map_err(|e| e.to_string())?;

    watcher
        .watch(&args.root, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    log::info!("initializing pool with {} workers ...", args.workers);

    let pool = ThreadPool::new(args.workers);

    log::info!("running ...");

    let engine = Arc::new(engine);

    // receive filesystem events
    loop {
        match rx.recv() {
            Ok(event) => match event {
                // we're interested in files creation and modification
                DebouncedEvent::Create(path)
                | DebouncedEvent::NoticeWrite(path)
                | DebouncedEvent::Write(path)
                | DebouncedEvent::Rename(_, path) => {
                    // if it's a file and it exists
                    if path.is_file() && path.exists() {
                        // create a reference to the engine
                        let an_engine = engine.clone();
                        // submit scan job to the threads pool
                        pool.execute(move || {
                            // perform the scanning
                            let res = an_engine.scan(&path);
                            if let Some(error) = res.error {
                                log::debug!("{:?}", error)
                            } else if res.detected {
                                log::warn!(
                                    "!!! MALWARE DETECTION: '{:?}' detected as '{:?}'",
                                    &path,
                                    res.tags.join(", ")
                                );
                            }
                        });
                    }
                }

                // ignored events
                DebouncedEvent::NoticeRemove(path) => {
                    log::trace!("ignoring remove event for {:?}", path);
                }
                DebouncedEvent::Chmod(path) => {
                    log::trace!("ignoring chmod event for {:?}", path);
                }
                DebouncedEvent::Remove(path) => {
                    log::trace!("ignoring remove event for {:?}", path);
                }
                // error events
                DebouncedEvent::Rescan => {
                    log::debug!("rescan");
                }
                DebouncedEvent::Error(error, maybe_path) => {
                    log::error!("error for {:?}: {:?}", maybe_path, error);
                }
            },
            Err(e) => log::error!("filesystem monitoring error: {:?}", e),
        }
    }
}
