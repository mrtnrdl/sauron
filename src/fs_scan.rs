use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use threadpool::ThreadPool;
use walkdir::WalkDir;

use crate::engine::Engine;
use crate::Arguments;

pub(crate) fn start(args: Arguments, engine: Engine) -> Result<(), String> {
    log::info!("initializing pool with {} workers ...", args.workers);

    let pool = ThreadPool::new(args.workers);

    log::info!("scanning {} ...", &args.root);

    let engine = Arc::new(engine);
    let start = Instant::now();
    let num_scanned = Arc::new(AtomicU32::new(0));
    let num_detected = Arc::new(AtomicU32::new(0));

    for entry in WalkDir::new(&args.root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let f_path = entry.path();
        let mut do_scan = args.ext.is_empty(); // init to true if not extensions were passed

        // do we have to filter by file extension?
        if !do_scan {
            if let Some(ext) = f_path.extension() {
                for filter_ext in &args.ext {
                    if filter_ext.to_lowercase() == *ext.to_string_lossy().to_lowercase() {
                        do_scan = true;
                        break;
                    }
                }
            }
        }

        if do_scan {
            // create thread-safe references
            let an_engine = engine.clone();
            let f_path = f_path.to_path_buf();
            let num_scanned = num_scanned.clone();
            let num_detected = num_detected.clone();

            // submit scan job to the threads pool
            pool.execute(move || {
                // perform the scanning
                let res = an_engine.scan(&f_path);
                if let Some(error) = res.error {
                    log::debug!("{:?}", error)
                } else if res.detected {
                    num_detected.fetch_add(1, Ordering::SeqCst);

                    log::warn!(
                        "!!! MALWARE DETECTION: '{:?}' detected as '{:?}'",
                        &f_path,
                        res.tags.join(", ")
                    );
                }

                num_scanned.fetch_add(1, Ordering::SeqCst);
            });
        }
    }

    pool.join();

    log::info!(
        "{:?} files scanned in {:?}, {:?} positive detections",
        num_scanned,
        start.elapsed(),
        num_detected
    );

    Ok(())
}
