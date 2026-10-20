//! Prints what the platform layer sees on this machine. Run by hand:
//! `cargo test -p notifi-platform --test listing -- --ignored --nocapture`

use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use notifi_platform::{DeviceDirection, Platform};

fn block_on<F: Future>(fut: F) -> F::Output {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut fut = std::pin::pin!(fut);
    loop {
        if let Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
            return out;
        }
        std::thread::park();
    }
}

#[test]
#[ignore = "reads the real machine; run by hand"]
fn list_everything() {
    let p = Platform::detect();
    println!("backend: {:?}", p.backend());
    for (f, s) in p.capabilities().iter() {
        println!("  cap {:<22} {:?}", f.name(), s);
    }

    let audio = p.audio();
    for dir in [DeviceDirection::Output, DeviceDirection::Input] {
        match audio.devices(dir) {
            Ok(list) => {
                for d in list {
                    println!("{dir:?} device: {:?} kind={:?} default={} vol={:.2} muted={} has_volume={}", d.name, d.kind, d.is_default, d.volume, d.muted, d.has_volume);
                }
            }
            Err(e) => println!("{dir:?} devices: {e}"),
        }
    }
    match audio.sessions() {
        Ok(list) => {
            for s in list {
                println!(
                    "session: {:?} app_id={:?} pid={:?} vol={:.2} muted={} peak={:.2} route={:?} icon={:?}",
                    s.app_name, s.app_id, s.pid, s.volume, s.muted, s.peak, s.device_id, s.icon_path
                );
            }
        }
        Err(e) => println!("sessions: {e}"),
    }

    // Round trip on the real output volume with the same value, so nothing audible changes.
    if let Ok(Some(out)) = audio.default_device(DeviceDirection::Output) {
        if out.has_volume {
            audio.set_device_volume(&out.id, out.volume).unwrap();
            let after = audio.default_device(DeviceDirection::Output).unwrap().unwrap();
            assert!((after.volume - out.volume).abs() < 0.01, "volume moved: {} -> {}", out.volume, after.volume);
            println!("volume round trip ok at {:.2}", out.volume);
        }
    }

    let display = p.display();
    match block_on(display.list()) {
        Ok(list) => {
            for d in list {
                let b = block_on(display.brightness(&d.id));
                println!(
                    "display: {} {:?} {:?} primary={} internal={} ddc={} scale={} brightness={:?}",
                    d.id, d.name, d.bounds, d.primary, d.internal, d.ddc, d.scale, b.map_err(|e| e.to_string())
                );
            }
        }
        Err(e) => println!("displays: {e}"),
    }

    match p.windows().list() {
        Ok(list) => {
            println!("{} windows", list.len());
            for w in list.iter().take(15) {
                println!("window: {:?} app={:?} pid={:?} {:?} monitor={:?} min={} focused={}", w.title, w.app_id, w.pid, w.bounds, w.monitor, w.minimized, w.focused);
            }
        }
        Err(e) => println!("windows: {e}"),
    }
    println!("foreground: {:?}", p.windows().foreground().map(|w| w.map(|w| w.title)));
    if let Ok(list) = block_on(display.list()) {
        for d in list {
            println!("foreground fullscreen on {}: {:?}", d.id, p.windows().foreground_fullscreen(d.bounds));
        }
    }
}
