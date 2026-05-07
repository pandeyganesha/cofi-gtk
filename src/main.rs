// ─────────────────────────────────────────────────────────────────────────────
// main.rs — cofi entry point
//
// This file does three things:
//   1. Defines the App struct that holds ALL state.
//   2. Implements the sctk (Smithay Client Toolkit) delegate traits — these are
//      the callbacks Wayland calls when something happens (screen resize, key
//      press, etc.).  They look like boilerplate because most of them do nothing;
//      only the ones we care about have real logic.
//   3. Sets up the Wayland layer-shell surface and runs the event loop.
//
// If you are new to Rust:
//   • `&mut self` means we can read AND write to `self`.
//   • `&self`     means read-only.
//   • `Option<T>` is either Some(value) or None (Rust has no null).
//   • `unwrap()`  crashes if the value is None — fine for prototyping.
//   • `expect("msg")` is the same but prints "msg" before crashing.
// ─────────────────────────────────────────────────────────────────────────────

mod config;
mod desktop;
mod layout;
mod lcs;
mod nav;
mod render;

// ── sctk imports ──────────────────────────────────────────────────────────────
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_registry,
    delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RepeatInfo},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use wayland_client::globals::registry_queue_init;

// ── wayland-client imports ────────────────────────────────────────────────────
use wayland_client::{
    protocol::{wl_keyboard::WlKeyboard, wl_output, wl_seat, wl_shm, wl_surface},
    Connection, QueueHandle,
};

// ── Cairo ─────────────────────────────────────────────────────────────────────
use cairo::{Format, ImageSurface};

// ── Event loop ────────────────────────────────────────────────────────────────
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;

// ─────────────────────────────────────────────────────────────────────────────
// Raw keysym constants
//
// We match on the u32 value of each key because it avoids depending on which
// exact version of the xkbcommon crate exposes which constants.
// These values come from <X11/keysymdef.h> and never change.
// ─────────────────────────────────────────────────────────────────────────────
const KEY_ESCAPE: u32 = 0xff1b;
const KEY_RETURN: u32 = 0xff0d;
const KEY_KP_ENTER: u32 = 0xff8d;
const KEY_BACKSPACE: u32 = 0xff08;
const KEY_UP: u32 = 0xff52;
const KEY_DOWN: u32 = 0xff54;
const KEY_LEFT: u32 = 0xff51;
const KEY_RIGHT: u32 = 0xff53;

// ─────────────────────────────────────────────────────────────────────────────
// App — the single struct that holds every piece of state
// ─────────────────────────────────────────────────────────────────────────────

struct App {
    // ── sctk state objects (required by the delegate traits) ─────────────────
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor_state: CompositorState,
    shm: Shm,
    layer_shell: LayerShell,

    // ── Wayland objects we own ────────────────────────────────────────────────
    layer_surface: Option<LayerSurface>, // the fullscreen overlay surface
    pool: Option<SlotPool>,              // shared memory pool (pixel data)
    keyboard: Option<WlKeyboard>,        // keyboard device handle

    // ── Screen dimensions ─────────────────────────────────────────────────────
    // Set by the compositor in the first `configure` callback.
    width: u32,
    height: u32,

    // ── Grid dimensions (logical, used for navigation) ────────────────────────
    cols: usize,
    rows: usize,

    // ── App data ──────────────────────────────────────────────────────────────
    apps: Vec<desktop::DesktopEntry>, // all desktop entries (shuffled)
    query: String,                    // what the user has typed
    visible: Vec<usize>,              // indices into `apps` that match `query`
    selected: Option<usize>,          // index into `apps` of the highlighted item

    // ── Per-app visual state ───────────────────────────────────────────────────
    /// Font size for each app.  Only ever grows while a query is active;
    /// reset to base_font_size when the query is cleared.
    app_sizes: Vec<f64>,
    /// Stable scatter positions (cx, cy) in pixels, computed once per screen size.
    app_positions: Vec<(f64, f64)>,
    /// The base font size for all apps at startup (all-apps view).
    base_font_size: f64,

    // ── Config / theme ────────────────────────────────────────────────────────
    config: config::Config,

    // ── Exit flag — set to true to break the event loop ───────────────────────
    exit: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// App methods — the actual application logic
// ─────────────────────────────────────────────────────────────────────────────

impl App {
    // ── Layout ────────────────────────────────────────────────────────────────

    /// Recalculate grid dimensions, scatter positions, and base font size.
    /// Called whenever the screen is configured (or resized).
    fn compute_layout(&mut self) {
        let (c, r) = layout::calculate_grid(self.apps.len(), self.width, self.height);
        self.cols = c;
        self.rows = r;

        // Stable scatter positions — recomputed any time the screen size changes.
        self.app_positions =
            layout::scatter_positions(self.apps.len(), self.width, self.height);

        // Base font size: generous startup size using 80th-percentile name.
        if !self.apps.is_empty() {
            let names: Vec<&str> = self.apps.iter().map(|a| a.name.as_str()).collect();
            self.base_font_size = layout::compute_base_font_size(
                &names,
                self.width,
                self.height,
                &self.config.theme.font_family,
                self.config.theme.min_font_size,
                self.config.theme.max_font_size,
            );
        }

        // (Re)initialise per-app sizes so they match the new base.
        self.app_sizes = vec![self.base_font_size; self.apps.len()];
    }

    // ── Filtering ─────────────────────────────────────────────────────────────

    /// Run the subsequence filter, update `self.visible`, grow matching app
    /// sizes toward the current match target, and fix up `self.selected`.
    fn update_filter(&mut self) {
        if self.query.is_empty() {
            // No query → show-all mode.  Reset sticky sizes.
            self.visible.clear();
            for s in self.app_sizes.iter_mut() {
                *s = self.base_font_size;
            }
            if self.selected.map_or(true, |s| s >= self.apps.len()) {
                self.selected = if self.apps.is_empty() { None } else { Some(0) };
            }
            return;
        }

        // Run the subsequence check on every app name.
        self.visible = self
            .apps
            .iter()
            .enumerate()
            .filter(|(_, app)| lcs::is_subsequence(&self.query, &app.name))
            .map(|(i, _)| i)
            .collect();

        // Compute the target size for matching apps (based on how many match).
        if !self.visible.is_empty() {
            let matching_names: Vec<&str> = self
                .visible
                .iter()
                .map(|&i| self.apps[i].name.as_str())
                .collect();

            let match_target = layout::compute_match_font_size(
                &matching_names,
                self.width,
                self.height,
                &self.config.theme.font_family,
                self.base_font_size,
                self.config.theme.max_font_size,
            );

            // Grow matching apps' sizes.  Sizes NEVER decrease.
            for &i in &self.visible {
                if match_target > self.app_sizes[i] {
                    self.app_sizes[i] = match_target;
                }
            }
            // Non-matching apps: their sizes stay at whatever peak they reached.
        }

        // Fix up selection.
        let current_still_visible =
            self.selected.map_or(false, |s| self.visible.contains(&s));
        if !current_still_visible {
            self.selected = self.visible.first().copied();
        }
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    /// Move `self.selected` in the given direction through the visible set.
    fn navigate(&mut self, dir: nav::Direction) {
        // Which apps can we navigate to right now?
        let navigatable: Vec<usize> = if self.query.is_empty() {
            // No query → all apps are navigatable.
            (0..self.apps.len()).collect()
        } else {
            self.visible.clone()
        };

        if navigatable.is_empty() {
            return;
        }

        // If current selection is outside the navigatable set, snap to first.
        let current = self
            .selected
            .filter(|s| navigatable.contains(s))
            .unwrap_or(navigatable[0]);

        self.selected = Some(nav::navigate(&navigatable, current, self.cols, dir));
    }

    // ── Launch ────────────────────────────────────────────────────────────────

    /// Launch the currently selected app (if any).
    fn launch_selected(&self) {
        if let Some(idx) = self.selected {
            if let Some(app) = self.apps.get(idx) {
                eprintln!("[cofi] launching: {}", app.exec);
                desktop::launch(&app.exec);
            }
        }
    }

    // ── Rendering ─────────────────────────────────────────────────────────────

    /// Render the current state to the Wayland surface.
    /// Called after every keystroke and on initial configure.
    fn draw(&mut self, qh: &QueueHandle<App>) {
        if self.width == 0 || self.height == 0 {
            return; // screen size not known yet
        }

        // ── Step 1: render with Cairo into a Vec<u8> ──────────────────────────
        //
        // We render into a fresh Cairo ImageSurface, then copy its pixel data
        // into the Wayland shared-memory buffer.  The extra copy (~8 MB for
        // 1080p) is fast enough for an interactive launcher.

        let mut cairo_surface =
            ImageSurface::create(Format::ARgb32, self.width as i32, self.height as i32)
                .expect("Failed to create Cairo surface");

        render::draw_frame(
            &cairo_surface,
            &self.apps,
            &self.visible,
            self.selected,
            &self.query,
            &self.app_sizes,
            &self.app_positions,
            self.base_font_size,
            self.width,
            self.height,
            &self.config,
        );

        cairo_surface.flush();

        // Copy the pixel data out into an owned Vec so we can drop the Cairo
        // surface and release the borrow before we touch `self.pool`.
        let pixels: Vec<u8> = {
            let data = cairo_surface
                .data()
                .expect("Failed to read Cairo surface data");
            data.to_vec() // owned copy
        };
        // cairo_surface (and its borrow of pixels) is dropped here.
        drop(cairo_surface);

        // ── Step 2: write pixels into the Wayland shm buffer ──────────────────
        //
        // SlotPool manages a chunk of shared memory.  Each call to
        // create_buffer() carves out a slot big enough for one frame.
        // The compositor releases the buffer back to us after it composites it.

        let pool = self.pool.as_mut().expect("Pool not initialised");

        let (buffer, canvas) = pool
            .create_buffer(
                self.width as i32,
                self.height as i32,
                self.width as i32 * 4, // stride = width × 4 bytes per pixel
                wl_shm::Format::Argb8888,
            )
            .expect("Failed to create Wayland buffer");

        canvas.copy_from_slice(&pixels);

        // canvas is a &mut [u8] that borrows from `pool`.
        // Explicitly drop it so the mutable borrow on pool ends here,
        // freeing us to borrow `self.layer_surface` on the next line.
        drop(canvas);

        // ── Step 3: attach buffer to the surface and commit ───────────────────
        //
        // After commit() Hyprland composites the buffer onto the screen.
        // `layer_surface` and `pool` are different fields of `self`, so Rust
        // allows us to borrow both (field-level borrow splitting).

        let wl_surf = self
            .layer_surface
            .as_ref()
            .expect("Layer surface not created")
            .wl_surface();

        buffer
            .attach_to(wl_surf)
            .expect("Failed to attach buffer to surface");

        wl_surf.damage_buffer(0, 0, self.width as i32, self.height as i32);
        wl_surf.commit();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// sctk delegate implementations
//
// These are the callbacks the Wayland event loop invokes.  Most are empty
// because cofi doesn't need to react to them.  The important ones are:
//   • LayerShellHandler::configure  — compositor tells us the screen size
//   • KeyboardHandler::press_key    — user typed something
// ─────────────────────────────────────────────────────────────────────────────

// ── Compositor (surface lifecycle) ───────────────────────────────────────────

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
}

// ── Output (monitor events) ───────────────────────────────────────────────────

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

// ── Layer Shell ───────────────────────────────────────────────────────────────

impl LayerShellHandler for App {
    /// The compositor sends `configure` to tell us what size we should be.
    /// For a fullscreen overlay anchored to all four edges this is the full
    /// monitor resolution.
    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        // Guard: compositor sometimes sends 0,0 on the very first configure.
        let w = configure.new_size.0;
        let h = configure.new_size.1;
        if w == 0 || h == 0 {
            return;
        }

        self.width = w;
        self.height = h;

        eprintln!("[cofi] screen: {w}×{h}");

        // (Re)create the shared memory pool sized for this screen.
        // ×2 so there is always a free slot while the compositor holds the
        // previous frame.
        let pool_bytes = (w as usize) * (h as usize) * 4 * 2;
        self.pool = Some(
            SlotPool::new(pool_bytes, &self.shm).expect("Failed to create shared memory pool"),
        );

        self.compute_layout();
        self.draw(qh);
    }

    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }
}

// ── Seat (input device management) ───────────────────────────────────────────

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    /// Called when a new input capability (keyboard, pointer, …) is announced.
    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        // We only care about the keyboard.
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let kb = self
                .seat_state
                .get_keyboard(qh, &seat, None) // None = use system keymap
                .expect("Failed to get keyboard");
            self.keyboard = Some(kb);
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            if let Some(kb) = self.keyboard.take() {
                kb.release();
            }
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

// ── Keyboard ──────────────────────────────────────────────────────────────────

impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: &wl_surface::WlSurface,
        _serial: u32,
        _raw: &[u32],
        _syms: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: &wl_surface::WlSurface,
        _serial: u32,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _serial: u32,
        _mods: Modifiers,
    ) {
    }

    fn update_repeat_info(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _info: RepeatInfo,
    ) {
    }

    /// This is where all the magic happens — every key press lands here.
    fn press_key(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        // We match on the raw u32 keysym value (see the KEY_* constants above).
        match event.keysym.raw() {
            // ── Quit without launching anything ───────────────────────────────
            KEY_ESCAPE => {
                self.exit = true;
            }

            // ── Launch selected app and exit ──────────────────────────────────
            KEY_RETURN | KEY_KP_ENTER => {
                // Only launch if the user has typed something AND something is
                // selected.  If nothing is typed we do nothing (as requested).
                if !self.query.is_empty() {
                    self.launch_selected();
                    self.exit = true;
                }
            }

            // ── Delete last character from query ──────────────────────────────
            KEY_BACKSPACE => {
                // pop() removes the last *char* (Unicode-safe).
                self.query.pop();
                self.update_filter();
                self.draw(qh);
            }

            // ── Arrow key navigation ──────────────────────────────────────────
            KEY_UP => {
                self.navigate(nav::Direction::Up);
                self.draw(qh);
            }
            KEY_DOWN => {
                self.navigate(nav::Direction::Down);
                self.draw(qh);
            }
            KEY_LEFT => {
                self.navigate(nav::Direction::Left);
                self.draw(qh);
            }
            KEY_RIGHT => {
                self.navigate(nav::Direction::Right);
                self.draw(qh);
            }

            // ── Any printable character → append to query ─────────────────────
            _ => {
                if let Some(text) = event.utf8 {
                    // `text` can be multi-byte (e.g. accented characters).
                    // Skip control characters like Tab.
                    if !text.chars().all(|c| c.is_control()) {
                        self.query.push_str(&text);
                        self.update_filter();
                        self.draw(qh);
                    }
                }
            }
        }
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _serial: u32,
        _event: KeyEvent,
    ) {
    }
}

// ── SHM ───────────────────────────────────────────────────────────────────────

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

// ── Registry ──────────────────────────────────────────────────────────────────

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    // Tells sctk to automatically update OutputState and SeatState from the
    // registry (so new monitors and seats are tracked for free).
    registry_handlers![OutputState, SeatState];
}

// ── Delegate macros ───────────────────────────────────────────────────────────
//
// These macros wire up all the Wayland Dispatch<Protocol, _> implementations
// that sctk needs internally.  Without them the event loop won't know how to
// route protocol events to our handler methods above.

delegate_compositor!(App);
delegate_output!(App);
delegate_layer!(App);
delegate_seat!(App);
delegate_keyboard!(App);
delegate_shm!(App);
delegate_registry!(App);

// ─────────────────────────────────────────────────────────────────────────────
// main()
// ─────────────────────────────────────────────────────────────────────────────

fn main() {
    // ── Connect to the Wayland compositor ────────────────────────────────────
    //
    // Reads $WAYLAND_DISPLAY (usually "wayland-0") and opens a socket to
    // Hyprland.  Fails loudly if the display is not set.
    let conn =
        Connection::connect_to_env().expect("Cannot connect to Wayland. Is $WAYLAND_DISPLAY set?");

    // ── Discover available global interfaces ──────────────────────────────────
    //
    // `registry_queue_init` sends a `wl_display.get_registry` and does a
    // blocking roundtrip so `globals` already contains everything by the time
    // we return.
    let (globals, event_queue) =
        registry_queue_init::<App>(&conn).expect("Failed to initialise Wayland registry");

    let qh = event_queue.handle();

    // ── Bind the protocols we need ────────────────────────────────────────────
    let compositor_state =
        CompositorState::bind(&globals, &qh).expect("wl_compositor not available");

    let layer_shell = LayerShell::bind(&globals, &qh)
        .expect("zwlr_layer_shell_v1 not available — is Hyprland running?");

    let shm = Shm::bind(&globals, &qh).expect("wl_shm not available");

    // ── Load apps and config ──────────────────────────────────────────────────
    let apps = desktop::load_apps();
    let config = config::Config::load();

    eprintln!("[cofi] loaded {} applications", apps.len());

    let initial_selected = if apps.is_empty() { None } else { Some(0) };

    // ── Build application state ───────────────────────────────────────────────
    let mut app = App {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        compositor_state,
        shm,
        layer_shell,

        layer_surface: None,
        pool: None,
        keyboard: None,

        width: 0,
        height: 0,
        cols: 1,
        rows: 1,

        apps,
        query: String::new(),
        visible: Vec::new(),
        selected: initial_selected,

        // These are properly initialised in compute_layout() once we know the
        // screen size.  Safe empty defaults here.
        app_sizes: Vec::new(),
        app_positions: Vec::new(),
        base_font_size: 12.0,

        config,
        exit: false,
    };

    // ── Create the layer-shell surface ────────────────────────────────────────
    //
    // A layer-shell surface is a special Wayland surface that can be pinned to
    // the screen edges and placed above (or below) normal windows.  We use the
    // OVERLAY layer so cofi appears above everything including other overlays.

    let wl_surface = app.compositor_state.create_surface(&qh);

    let layer_surface = app.layer_shell.create_layer_surface(
        &qh,
        wl_surface,
        Layer::Overlay, // above all normal windows
        Some("cofi"),   // namespace — used in hyprland.conf layerrule
        None,           // output: None = the focused monitor
    );

    // Anchor to all four edges → the compositor sizes us to fill the monitor.
    layer_surface.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer_surface.set_size(0, 0); // 0,0 = let compositor decide
    layer_surface.set_exclusive_zone(-1); // don't push panels away
    layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);

    // Commit the surface so the compositor sends us a `configure` event with
    // the actual screen dimensions.
    layer_surface.commit();
    app.layer_surface = Some(layer_surface);

    // ── Set up the event loop ─────────────────────────────────────────────────
    //
    // calloop is an epoll-based event loop.  WaylandSource bridges the Wayland
    // socket into calloop so both can share the same thread.

    let mut event_loop: EventLoop<App> = EventLoop::try_new().expect("Failed to create event loop");

    WaylandSource::new(conn, event_queue)
        .insert(event_loop.handle())
        .expect("Failed to add Wayland source to event loop");

    // ── Run ───────────────────────────────────────────────────────────────────
    //
    // `dispatch(None, &mut app)` blocks until at least one event fires, then
    // calls all the relevant handler methods above.  We loop until `app.exit`
    // is set to true.

    loop {
        event_loop
            .dispatch(None, &mut app)
            .expect("Event loop error");

        if app.exit {
            break;
        }
    }
}
