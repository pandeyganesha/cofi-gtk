use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, DrawingArea, EventControllerKey};
use gtk::gdk;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;

mod config;
mod desktop;
mod layout;
mod lcs;
mod nav;
mod render;

struct AppState {
    width: u32,
    height: u32,
    cols: usize,
    rows: usize,
    apps: Vec<desktop::DesktopEntry>,
    query: String,
    visible: Vec<usize>,
    selected: Option<usize>,
    app_sizes: Vec<f64>,
    app_positions: Vec<(f64, f64)>,
    base_font_size: f64,
    config: config::Config,
}

impl AppState {
    fn new() -> Self {
        let apps = desktop::load_apps();
        let config = config::Config::load();
        let selected = if apps.is_empty() { None } else { Some(0) };

        Self {
            width: 0,
            height: 0,
            cols: 1,
            rows: 1,
            apps,
            query: String::new(),
            visible: Vec::new(),
            selected,
            app_sizes: Vec::new(),
            app_positions: Vec::new(),
            base_font_size: 12.0,
            config,
        }
    }

    fn compute_layout(&mut self) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        
        let (c, r) = layout::calculate_grid(self.apps.len(), self.width, self.height);
        self.cols = c;
        self.rows = r;

        let raw_positions = layout::scatter_positions(self.apps.len(), self.width, self.height);
        
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
            self.app_positions = layout::settle_positions(
                &names,
                &raw_positions,
                self.base_font_size,
                &self.config.theme.font_family,
                self.width,
                self.height,
            );
        } else {
            self.app_positions = raw_positions;
        }
        
        self.app_sizes = vec![self.base_font_size; self.apps.len()];
        self.update_filter();
    }

    fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.visible.clear();
            for s in self.app_sizes.iter_mut() {
                *s = self.base_font_size;
            }
            if self.selected.map_or(true, |s| s >= self.apps.len()) {
                self.selected = if self.apps.is_empty() { None } else { Some(0) };
            }
            return;
        }

        self.visible = self
            .apps
            .iter()
            .enumerate()
            .filter(|(_, app)| lcs::is_subsequence(&self.query, &app.name))
            .map(|(i, _)| i)
            .collect();

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
            for &i in &self.visible {
                if match_target > self.app_sizes[i] {
                    self.app_sizes[i] = match_target;
                }
            }
        }

        if self.visible.is_empty() {
            self.selected = None;
        } else {
            let query_lower = self.query.to_lowercase();
            let best = self.visible.iter().copied().max_by_key(|&i| {
                let name = self.apps[i].name.to_lowercase();
                let score = if name.starts_with(&query_lower) {
                    3
                } else if name.contains(&query_lower) {
                    2
                } else {
                    1
                };
                (score, -(i as isize))
            });
            self.selected = best;
        }
    }

    fn navigate(&mut self, dir: nav::Direction) {
        let navigatable: Vec<usize> = if self.query.is_empty() {
            (0..self.apps.len()).collect()
        } else {
            self.visible.clone()
        };

        if navigatable.is_empty() {
            return;
        }

        let current = self
            .selected
            .filter(|s| navigatable.contains(s))
            .unwrap_or(navigatable[0]);

        self.selected = Some(nav::navigate(
            &navigatable,
            current,
            &self.app_positions,
            self.base_font_size,
            dir,
        ));
    }

    fn launch_selected(&self) {
        if let Some(idx) = self.selected {
            if let Some(app) = self.apps.get(idx) {
                eprintln!("[cofi] launching: {}", app.exec);
                desktop::launch(&app.exec);
            }
        }
    }
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("cofi")
        .decorated(false)
        .fullscreened(true)
        .build();

    let provider = gtk::CssProvider::new();
    provider.load_from_data("window { background: transparent; }");
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("Could not connect to a display."),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let state = Rc::new(RefCell::new(AppState::new()));
    let drawing_area = DrawingArea::new();
    drawing_area.set_focusable(true);
    drawing_area.grab_focus();

    let state_draw = state.clone();
    drawing_area.set_draw_func(move |_area, cr, width, height| {
        let mut s = state_draw.borrow_mut();
        if s.width != width as u32 || s.height != height as u32 {
            s.width = width as u32;
            s.height = height as u32;
            s.compute_layout();
        }

        render::draw_frame(
            cr,
            &s.apps,
            &s.visible,
            s.selected,
            &s.query,
            &s.app_sizes,
            &s.app_positions,
            s.base_font_size,
            s.width,
            s.height,
            &s.config,
        );
    });

    let state_key = state.clone();
    let window_weak = window.downgrade();
    
    let key_controller = EventControllerKey::new();
    key_controller.connect_key_pressed(move |controller, keyval, _keycode, _state| {
        let mut s = state_key.borrow_mut();
        let mut handled = true;

        match keyval {
            gdk::Key::Escape => {
                if let Some(w) = window_weak.upgrade() {
                    w.close();
                }
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                if !s.query.is_empty() {
                    s.launch_selected();
                    if let Some(w) = window_weak.upgrade() {
                        w.close();
                    }
                }
            }
            gdk::Key::BackSpace => {
                s.query.pop();
                s.update_filter();
                controller.widget().unwrap().queue_draw();
            }
            gdk::Key::Up => {
                s.navigate(nav::Direction::Up);
                controller.widget().unwrap().queue_draw();
            }
            gdk::Key::Down => {
                s.navigate(nav::Direction::Down);
                controller.widget().unwrap().queue_draw();
            }
            _ => {
                if let Some(c) = keyval.to_unicode() {
                    if !c.is_control() {
                        s.query.push(c);
                        s.update_filter();
                        controller.widget().unwrap().queue_draw();
                    } else {
                        handled = false;
                    }
                } else {
                    handled = false;
                }
            }
        }
        
        if handled {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });

    drawing_area.add_controller(key_controller);
    window.set_child(Some(&drawing_area));
    window.present();
}

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id("com.github.pandeyganesha.cofi")
        .build();

    app.connect_activate(build_ui);
    app.run()
}
