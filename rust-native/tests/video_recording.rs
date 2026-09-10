use std::{
    fs,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use rustwright::{chromium, ActionOptions, GotoOptions, LaunchOptions, VideoOptions};

fn launch() -> Option<rustwright::Browser> {
    if chromium().executable_path().is_none() {
        eprintln!("skipping video recording test: Chromium executable unavailable");
        return None;
    }
    Some(
        chromium()
            .launch(LaunchOptions::default())
            .expect("launch browser"),
    )
}

#[test]
fn page_screencast_records_webm() {
    let Some(browser) = launch() else {
        return;
    };
    let page = browser.new_page().expect("new page");
    page.goto(
        "data:text/html,<title>record</title><h1 id=ok>ready</h1>",
        GotoOptions::default().wait_until("load").timeout(10_000.0),
    )
    .expect("goto");
    let output = std::env::temp_dir().join(format!(
        "rustwright-video-live-{}-{}.webm",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    page.start_video(output.to_string_lossy().as_ref(), VideoOptions::default())
        .expect("start video");
    assert!(page.is_recording_video());
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let _ = page.evaluate(
            "document.getElementById('ok').textContent = Date.now().toString()",
            None,
            ActionOptions::default(),
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let recording = page.stop_video().expect("stop video");
    assert!(!page.is_recording_video());
    assert!(
        recording.frames >= 1,
        "expected at least one screencast frame, got {}",
        recording.frames
    );
    assert_eq!(recording.path, output.to_string_lossy());
    let bytes = fs::read(&output).expect("read webm");
    assert_eq!(&bytes[0..4], &[0x1A, 0x45, 0xDF, 0xA3]);
    assert!(bytes.windows(5).any(|window| window == b"V_VP8"));
    let _ = fs::remove_file(output);
    page.close(Default::default()).expect("close page");
    browser.close().expect("close browser");
}
