//! Files dragged onto the window from a file manager. winit reports them on
//! X11 and Windows; on Wayland, where winit has no drag and drop, a data
//! device of ReCore's own on winit's connection does.

use std::path::PathBuf;

/// A drag of files over the window.
#[cfg_attr(
    windows,
    allow(
        dead_code,
        reason = "on Windows the window has iced's own drag events and asks only where the pointer is"
    )
)]
pub enum Drag {
    /// Files came over the window.
    Entered,
    /// The pointer dragging them is here, in the window's logical pixels as
    /// the platform has them, before ReCore's zoom.
    Moved { x: f64, y: f64 },
    /// They left it, or the drag ended elsewhere.
    Left,
    /// They were let go over it.
    Dropped(Vec<PathBuf>),
}

/// Where the pointer is over a window, for the drags winit reports without
/// it: on X11 and Windows. One is made for each drag and asked while the
/// files hover.
pub struct Pointer {
    source: Source,
    /// The window's scale, from its physical pixels to its logical ones.
    scale: f64,
}

enum Source {
    /// A connection of its own, and the window.
    #[cfg(target_os = "linux")]
    X11(x11rb::rust_connection::RustConnection, u32),
    #[cfg(windows)]
    Win32(isize),
    /// No pointer-tracking surface on this platform; `of` never makes one.
    #[cfg(not(any(target_os = "linux", windows)))]
    Unsupported,
}

impl Pointer {
    /// For `window`, whose physical pixels are `scale` of its logical ones.
    pub fn of(
        window: &(impl raw_window_handle::HasWindowHandle + ?Sized),
        scale: f64,
    ) -> Option<Self> {
        use raw_window_handle::RawWindowHandle;
        let handle = window.window_handle().ok()?.as_raw();
        let source = match handle {
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xlib(handle) => Self::x11(handle.window as u32)?,
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xcb(handle) => Self::x11(handle.window.get())?,
            #[cfg(windows)]
            RawWindowHandle::Win32(handle) => Source::Win32(handle.hwnd.get()),
            _ => return None,
        };
        Some(Self { source, scale })
    }

    #[cfg(target_os = "linux")]
    fn x11(window: u32) -> Option<Source> {
        match x11rb::connect(None) {
            Ok((connection, _)) => Some(Source::X11(connection, window)),
            Err(error) => {
                tracing::warn!(%error, "could not follow files dragged over the window");
                None
            }
        }
    }

    /// Where it is now, unless the platform won't say.
    pub fn at(&self) -> Option<Drag> {
        let (x, y): (f64, f64) = match &self.source {
            #[cfg(target_os = "linux")]
            Source::X11(connection, window) => {
                use x11rb::protocol::xproto::ConnectionExt;
                let pointer = connection.query_pointer(*window).ok()?.reply().ok()?;
                (f64::from(pointer.win_x), f64::from(pointer.win_y))
            }
            #[cfg(windows)]
            Source::Win32(hwnd) => {
                use windows_sys::Win32::Foundation::POINT;
                use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
                use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
                let mut point = POINT { x: 0, y: 0 };
                // SAFETY: the point outlives both calls, and a window gone
                // meanwhile makes ScreenToClient fail rather than misbehave.
                if unsafe { GetCursorPos(&mut point) } == 0
                    || unsafe { ScreenToClient(*hwnd as _, &mut point) } == 0
                {
                    return None;
                }
                (f64::from(point.x), f64::from(point.y))
            }
            #[cfg(not(any(target_os = "linux", windows)))]
            Source::Unsupported => return None,
        };
        Some(Drag::Moved {
            x: x / self.scale,
            y: y / self.scale,
        })
    }
}

#[cfg(target_os = "linux")]
pub mod wayland {
    //! A `wl_data_device` for the seat, on winit's `wl_display` but on a
    //! queue of its own, which a thread dispatches: winit reads the socket
    //! and libwayland queues these events for the thread. It takes
    //! `text/uri-list` on a drop. There's a device for each seat, not for a
    //! window, so a window made again needs nothing new.

    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::time::Duration;

    use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
    use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
    use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
    use wayland_client::protocol::wl_registry::{self, WlRegistry};
    use wayland_client::protocol::wl_seat::WlSeat;
    use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};

    use super::Drag;

    const URI_LIST: &str = "text/uri-list";
    const IMAGES: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];
    /// Which kinds an offer has: `text/uri-list` and the image types.
    const URIS: u8 = 1;
    /// The most a dropped or downloaded picture may be, in bytes.
    const LARGEST: u64 = 25 * 1024 * 1024;

    fn image_bit(index: usize) -> u8 {
        1 << (index + 1)
    }

    fn image_mime(bits: u8) -> Option<&'static str> {
        IMAGES
            .iter()
            .enumerate()
            .find(|(index, _)| bits & image_bit(*index) != 0)
            .map(|(_, mime)| *mime)
    }

    /// What a dropped offer holds: files, or a picture's bytes, and the
    /// image to fall back to when a URI list yields no file.
    #[derive(Clone, Copy)]
    enum Choice {
        Uris { image: Option<&'static str> },
        Image(&'static str),
    }

    /// Watches for files dragged over ReCore's windows on the connection whose
    /// `wl_display` this is, reporting each step to `report` from a thread.
    ///
    /// # Safety
    ///
    /// `display` is a live `wl_display`, which stays connected while the
    /// process runs, as winit's does for its event loop's life.
    pub unsafe fn watch(
        display: std::ptr::NonNull<std::ffi::c_void>,
        report: impl Fn(Drag) + Send + 'static,
    ) {
        let display = display.as_ptr() as usize;
        let thread = std::thread::Builder::new()
            .name("recore-drop".into())
            // It keeps little: the device, an offer, and a dropped list.
            .stack_size(128 * 1024)
            .spawn(move || {
                // SAFETY: the caller keeps the display connected.
                let backend = unsafe {
                    wayland_backend::client::Backend::from_foreign_display(display as *mut _)
                };
                let connection = Connection::from_backend(backend);
                tracing::debug!("watching for files dropped on the window");
                let mut queue = connection.new_event_queue();
                connection.display().get_registry(&queue.handle(), ());
                let mut state = State {
                    report: Box::new(report),
                    connection,
                    manager: None,
                    seats: Vec::new(),
                    offer: None,
                    left: None,
                };
                loop {
                    if let Err(error) = queue.blocking_dispatch(&mut state) {
                        tracing::warn!(%error, "files can no longer be dropped on the window");
                        return;
                    }
                }
            });
        if let Err(error) = thread {
            tracing::warn!(%error, "could not watch for files dropped on the window");
        }
    }

    struct State {
        report: Box<dyn Fn(Drag) + Send>,
        connection: Connection,
        manager: Option<WlDataDeviceManager>,
        /// Seats announced before the manager, waiting for it.
        seats: Vec<WlSeat>,
        /// The offer over a window, and what it holds.
        offer: Option<(WlDataOffer, Choice)>,
        /// The offer a drag left with, kept until the next drag comes: Weston
        /// cancels a drag whose last offer is destroyed before another
        /// client's takes its place.
        left: Option<WlDataOffer>,
    }

    /// Which MIME types an offer has, as [`URIS`] and [`image_bit`] bits.
    #[derive(Default)]
    struct Offered(AtomicU8);

    impl Dispatch<WlRegistry, ()> for State {
        fn event(
            state: &mut Self,
            registry: &WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            queue: &QueueHandle<Self>,
        ) {
            let wl_registry::Event::Global {
                name,
                interface,
                version,
            } = event
            else {
                return;
            };
            match interface.as_str() {
                // Version 3 for the actions a drop needs agreed.
                "wl_data_device_manager" if state.manager.is_none() => {
                    state.manager = Some(registry.bind(name, version.min(3), queue, ()));
                }
                "wl_seat" => state.seats.push(registry.bind(name, 1, queue, ())),
                _ => return,
            }
            if let Some(manager) = &state.manager {
                for seat in state.seats.drain(..) {
                    manager.get_data_device(&seat, queue, ());
                }
            }
        }
    }

    impl Dispatch<WlDataDevice, ()> for State {
        fn event(
            state: &mut Self,
            _: &WlDataDevice,
            event: wl_data_device::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                wl_data_device::Event::Enter {
                    serial,
                    x,
                    y,
                    id: Some(offer),
                    ..
                } => {
                    let bits = offer
                        .data::<Offered>()
                        .map_or(0, |offered| offered.0.load(Ordering::Relaxed));
                    let choice = if bits & URIS != 0 {
                        Some(Choice::Uris {
                            image: image_mime(bits),
                        })
                    } else {
                        image_mime(bits).map(Choice::Image)
                    };
                    let old = if let Some(choice) = choice {
                        let mime = match choice {
                            Choice::Uris { .. } => URI_LIST,
                            Choice::Image(mime) => mime,
                        };
                        offer.accept(serial, Some(mime.into()));
                        if offer.version() >= 3 {
                            offer.set_actions(DndAction::Copy, DndAction::Copy);
                        }
                        // Where first, so the window shows the right place.
                        (state.report)(Drag::Moved { x, y });
                        (state.report)(Drag::Entered);
                        state.offer.replace((offer, choice))
                    } else {
                        offer.accept(serial, None);
                        offer.destroy();
                        None
                    };
                    for old in [old.map(|(old, _)| old), state.left.take()]
                        .into_iter()
                        .flatten()
                    {
                        old.destroy();
                    }
                }
                wl_data_device::Event::Motion { x, y, .. } => {
                    if state.offer.is_some() {
                        (state.report)(Drag::Moved { x, y });
                    }
                }
                wl_data_device::Event::Leave => {
                    if let Some((offer, _choice)) = state.offer.take() {
                        state.left = Some(offer);
                        (state.report)(Drag::Left);
                    }
                }
                wl_data_device::Event::Drop => {
                    if let Some((offer, choice)) = state.offer.take() {
                        let paths = state.receive(&offer, choice);
                        if offer.version() >= 3 {
                            offer.finish();
                        }
                        offer.destroy();
                        if !paths.is_empty() {
                            (state.report)(Drag::Dropped(paths));
                        }
                    }
                }
                // The clipboard's offers: ReCore reads the clipboard elsewhere.
                wl_data_device::Event::Selection { id: Some(offer) } => offer.destroy(),
                _ => {}
            }
        }

        event_created_child!(State, WlDataDevice, [
            wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, Offered::default()),
        ]);
    }

    impl State {
        /// What a dropped offer holds, read as the source sends it. A URI
        /// list gives this machine's files, and the pictures its addresses
        /// lead to; an image type gives its bytes as a temporary file.
        fn receive(&self, offer: &WlDataOffer, choice: Choice) -> Vec<std::path::PathBuf> {
            match choice {
                Choice::Uris { image } => {
                    let list = self.receive_text(offer, URI_LIST);
                    let mut files = crate::paste::files(&list);
                    files.extend(downloaded(&list));
                    if files.is_empty()
                        && let Some(mime) = image
                        && let Some(path) = self.receive_image(offer, mime)
                    {
                        files.push(path);
                    }
                    files
                }
                Choice::Image(mime) => self.receive_image(offer, mime).into_iter().collect(),
            }
        }

        fn receive_text(&self, offer: &WlDataOffer, mime: &str) -> String {
            let Some(mut reader) = self.ask(offer, mime) else {
                return String::new();
            };
            let mut text = String::new();
            if let Err(error) = reader.read_to_string(&mut text) {
                tracing::warn!(%error, "could not read what was dropped");
            }
            text
        }

        /// A picture's bytes, written to a temporary file the upload takes
        /// over; the drop thread may block while the source sends them.
        fn receive_image(&self, offer: &WlDataOffer, mime: &str) -> Option<std::path::PathBuf> {
            let mut reader = self.ask(offer, mime)?;
            let mut bytes = Vec::new();
            if Read::by_ref(&mut reader)
                .take(LARGEST + 1)
                .read_to_end(&mut bytes)
                .is_err()
                || bytes.is_empty()
                || bytes.len() as u64 > LARGEST
            {
                return None;
            }
            let extension = match mime {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                "image/gif" => "gif",
                _ => "webp",
            };
            let file = tempfile::Builder::new()
                .prefix("recore-drop-")
                .suffix(&format!(".{extension}"))
                .tempfile()
                .ok()?;
            let (mut file, path) = file.keep().ok()?;
            file.write_all(&bytes).ok()?;
            Some(path)
        }

        fn ask(&self, offer: &WlDataOffer, mime: &str) -> Option<std::io::PipeReader> {
            let (reader, writer) = std::io::pipe().ok()?;
            offer.receive(mime.into(), std::os::fd::AsFd::as_fd(&writer));
            drop(writer);
            if let Err(error) = self.connection.flush() {
                tracing::warn!(%error, "could not ask for what was dropped");
                return None;
            }
            Some(reader)
        }
    }

    /// The pictures among a URI list's addresses, fetched and saved beside
    /// the session's other dropped files.
    fn downloaded(list: &str) -> Vec<std::path::PathBuf> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("ReCore")
            .timeout(Duration::from_secs(10))
            .build();
        let Ok(client) = client else {
            return Vec::new();
        };
        let mut paths = Vec::new();
        for address in list
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with('#'))
            .filter(|line| line.starts_with("http://") || line.starts_with("https://"))
        {
            let Some(extension) = image_extension(address) else {
                continue;
            };
            let Ok(mut response) = client.get(address).send() else {
                continue;
            };
            if !response.status().is_success() {
                continue;
            }
            let picture = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("image/"));
            if !picture {
                continue;
            }
            let mut bytes = Vec::new();
            if Read::by_ref(&mut response)
                .take(LARGEST + 1)
                .read_to_end(&mut bytes)
                .is_err()
                || bytes.is_empty()
                || bytes.len() as u64 > LARGEST
            {
                continue;
            }
            let Ok(file) = tempfile::Builder::new()
                .prefix("recore-drop-")
                .suffix(&format!(".{extension}"))
                .tempfile()
            else {
                continue;
            };
            let Ok((mut file, path)) = file.keep() else {
                continue;
            };
            if file.write_all(&bytes).is_ok() {
                paths.push(path);
            }
        }
        paths
    }

    fn image_extension(address: &str) -> Option<&'static str> {
        let path = address.split(['?', '#']).next()?.to_ascii_lowercase();
        if path.ends_with(".png") {
            Some("png")
        } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
            Some("jpg")
        } else if path.ends_with(".gif") {
            Some("gif")
        } else if path.ends_with(".webp") {
            Some("webp")
        } else {
            None
        }
    }

    impl Dispatch<WlDataOffer, Offered> for State {
        fn event(
            _: &mut Self,
            _: &WlDataOffer,
            event: wl_data_offer::Event,
            offered: &Offered,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let wl_data_offer::Event::Offer { mime_type } = event {
                let bit = if mime_type == URI_LIST {
                    URIS
                } else if let Some(index) = IMAGES.iter().position(|mime| *mime == mime_type) {
                    image_bit(index)
                } else {
                    0
                };
                if bit != 0 {
                    offered.0.fetch_or(bit, Ordering::Relaxed);
                }
            }
        }
    }

    impl Dispatch<WlDataDeviceManager, ()> for State {
        fn event(
            _: &mut Self,
            _: &WlDataDeviceManager,
            _: <WlDataDeviceManager as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<WlSeat, ()> for State {
        fn event(
            _: &mut Self,
            _: &WlSeat,
            _: <WlSeat as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    #[cfg(test)]
    mod tests {
        use super::image_extension;

        #[test]
        fn a_picture_address_names_its_type() {
            assert_eq!(
                image_extension("https://cdn.example/a.PNG?size=2"),
                Some("png")
            );
            assert_eq!(image_extension("https://cdn.example/a.jpeg#x"), Some("jpg"));
            assert_eq!(image_extension("https://cdn.example/a.gif"), Some("gif"));
            assert_eq!(image_extension("https://cdn.example/a.webp"), Some("webp"));
            assert_eq!(image_extension("https://cdn.example/a.html"), None);
        }
    }
}
