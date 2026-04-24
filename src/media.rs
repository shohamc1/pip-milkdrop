use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

static IS_PLAYING: AtomicBool = AtomicBool::new(false);

pub fn is_media_playing() -> bool {
    IS_PLAYING.load(Ordering::Relaxed)
}

pub fn start_polling() {
    let queue = dispatch2::DispatchQueue::new("com.pip-milkdrop.media", None);
    std::thread::spawn(move || {
        loop {
            poll_once(&queue);
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

fn poll_once(queue: &dispatch2::DispatchQueue) {
    let result = Arc::new(AtomicBool::new(false));
    let result_clone = result.clone();

    let block = block2::RcBlock::new(move |is_playing: objc2::runtime::Bool| {
        result_clone.store(is_playing.as_bool(), Ordering::Relaxed);
    });

    unsafe {
        MRMediaRemoteGetNowPlayingApplicationIsPlaying(
            queue as *const _ as *mut std::ffi::c_void,
            block2::RcBlock::into_raw(block) as *mut std::ffi::c_void,
        );
    }

    std::thread::sleep(Duration::from_millis(200));
    IS_PLAYING.store(result.load(Ordering::Relaxed), Ordering::Relaxed);
}

#[link(name = "MediaRemote", kind = "framework")]
extern "C" {
    fn MRMediaRemoteGetNowPlayingApplicationIsPlaying(
        queue: *mut std::ffi::c_void,
        block: *mut std::ffi::c_void,
    );
}
