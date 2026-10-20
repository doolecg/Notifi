//! OS capability layer. One trait per capability, one backend per OS/session,
//! chosen at runtime by [`Platform::detect`].

mod backend;
mod caps;
mod common;
mod error;
mod media;
mod traits;

#[cfg(windows)]
mod win;
#[cfg(windows)]
mod win_media;

#[cfg(target_os = "linux")]
mod gnome;
#[cfg(target_os = "linux")]
mod kde;
#[cfg(target_os = "linux")]
mod linux_common;
#[cfg(target_os = "linux")]
mod linux_media;
#[cfg(target_os = "linux")]
mod x11;

use std::sync::Arc;

pub use backend::{Backend, SessionEnv};
pub use caps::{Capabilities, Feature, Support};
pub use common::{EventStream, Guard, Image, Rect};
pub use error::{PlatformError, Result};
pub use traits::*;

trait AllCapabilities:
    Audio + WindowManager + Input + Clipboard + Display + Power + Capture + Packages + Sensors + Session + Media + 'static
{
}

impl<T> AllCapabilities for T where
    T: Audio + WindowManager + Input + Clipboard + Display + Power + Capture + Packages + Sensors + Session + Media + 'static
{
}

#[derive(Clone)]
pub struct Platform {
    backend: Backend,
    capabilities: Capabilities,
    audio: Arc<dyn Audio>,
    windows: Arc<dyn WindowManager>,
    input: Arc<dyn Input>,
    clipboard: Arc<dyn Clipboard>,
    display: Arc<dyn Display>,
    power: Arc<dyn Power>,
    capture: Arc<dyn Capture>,
    packages: Arc<dyn Packages>,
    sensors: Arc<dyn Sensors>,
    session: Arc<dyn Session>,
    media: Arc<dyn Media>,
}

impl Platform {
    pub fn detect() -> Self {
        let backend = Backend::detect();
        tracing::info!(?backend, "platform backend selected");
        Self::for_backend(backend)
    }

    fn for_backend(backend: Backend) -> Self {
        #[cfg(windows)]
        if backend == Backend::Windows {
            return Self::build(backend, win::capabilities(), win::Win);
        }
        #[cfg(target_os = "linux")]
        if backend != Backend::Windows && backend != Backend::Other {
            return Self::build(backend, linux_common::capabilities(backend), linux_common::Linux::new(backend));
        }
        Self::build(backend, Capabilities::not_implemented(), Null)
    }

    fn build<T: AllCapabilities>(backend: Backend, capabilities: Capabilities, imp: T) -> Self {
        let imp = Arc::new(imp);
        Self {
            backend,
            capabilities,
            audio: imp.clone(),
            windows: imp.clone(),
            input: imp.clone(),
            clipboard: imp.clone(),
            display: imp.clone(),
            power: imp.clone(),
            capture: imp.clone(),
            packages: imp.clone(),
            sensors: imp.clone(),
            session: imp.clone(),
            media: imp,
        }
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }
    pub fn audio(&self) -> &dyn Audio {
        &*self.audio
    }
    pub fn windows(&self) -> &dyn WindowManager {
        &*self.windows
    }
    pub fn input(&self) -> &dyn Input {
        &*self.input
    }
    pub fn clipboard(&self) -> &dyn Clipboard {
        &*self.clipboard
    }
    pub fn display(&self) -> &dyn Display {
        &*self.display
    }
    pub fn power(&self) -> &dyn Power {
        &*self.power
    }
    pub fn capture(&self) -> &dyn Capture {
        &*self.capture
    }
    pub fn packages(&self) -> &dyn Packages {
        &*self.packages
    }
    pub fn sensors(&self) -> &dyn Sensors {
        &*self.sensors
    }
    pub fn session(&self) -> &dyn Session {
        &*self.session
    }
    pub fn media(&self) -> &dyn Media {
        &*self.media
    }
}

/// Backend for an OS Notifi doesn't support: every call is `Unsupported`.
struct Null;

impl Audio for Null {}
impl WindowManager for Null {}
impl Input for Null {}
impl Clipboard for Null {}
impl Display for Null {}
impl Power for Null {}
impl Capture for Null {}
impl Packages for Null {}
impl Sensors for Null {}
impl Session for Null {}
impl Media for Null {}

#[cfg(test)]
pub(crate) fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    use std::task::{Context, Poll, Wake, Waker};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_backend_reports_unsupported() {
        let p = Platform::for_backend(Backend::Other);
        let err = p.audio().sessions().unwrap_err();
        assert!(matches!(err, PlatformError::Unsupported { feature: Feature::AudioSessions, .. }));
        let err = block_on(p.session().dark_mode()).unwrap_err();
        assert!(matches!(err, PlatformError::Unsupported { feature: Feature::DarkModeRead, .. }));
        assert!(matches!(p.capabilities().get(Feature::KeepAwake), Support::Unsupported(_)));
    }

    #[test]
    fn capabilities_cover_every_feature() {
        let p = Platform::detect();
        for f in Feature::ALL {
            assert!(p.capabilities().iter().any(|(g, _)| g == *f), "{} missing", f.name());
        }
    }

    #[test]
    fn capabilities_set_overrides() {
        let mut caps = Capabilities::not_implemented();
        caps.set(Feature::InputGrab, Support::NeedsPermission("input group".into()));
        assert_eq!(caps.get(Feature::InputGrab), &Support::NeedsPermission("input group".into()));
    }

    #[test]
    fn unsupported_error_names_the_feature() {
        let e = PlatformError::unsupported(Feature::DisplayDdc, "no DDC on this panel");
        assert_eq!(e.to_string(), "display-ddc is not supported here: no DDC on this panel");
    }
}
