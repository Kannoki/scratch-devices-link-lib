//! Main application state and eframe integration for Future Academy Link GUI.
//!
//! This implements the UI design from Figma:
//! - Primary blue background (#0e69b3)
//! - White content area with rounded corners (36px radius)
//! - Device cards with blue shadow header and gradient
//! - Float button bar at bottom

use crate::gui::screens::{DeviceInfo, Screen};
use crate::gui::styles::Colors;
use eframe::egui;
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// Application state shared between GUI and backend
#[derive(Clone)]
pub struct AppState {
    pub screen: Screen,
    pub rotation: f32,
    pub last_update: Instant,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            screen: Screen::Starting,
            rotation: 0.0,
            last_update: Instant::now(),
        }
    }
}

/// Thread-safe state wrapper
pub type SharedState = Arc<RwLock<AppState>>;

/// Main application struct for eframe
pub struct FutureAcademyApp {
    state: SharedState,
}

impl FutureAcademyApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, state: SharedState) -> Self {
        Self { state }
    }

    /// Draw the UI content
    fn draw_ui(&mut self, ui: &mut egui::Ui, state: &AppState, rotation: f32) {
        // Header with logo and title
        ui.horizontal(|ui| {
            // Logo placeholder - white circle
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(36.0, 36.0),
                egui::Sense::hover()
            );
            ui.painter().circle_filled(rect.center(), 18.0, Colors::WHITE);

            ui.add_space(13.35);

            // Title
            ui.label(egui::RichText::new("FUTURE ").color(Colors::WHITE).size(24.0).strong());
            ui.label(egui::RichText::new("ACADEMY").color(Colors::WHITE).size(24.0).strong());
            
            // Settings and Controller buttons on the right
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                // Controller button (placeholder circle)
                let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 15.0, Colors::WHITE);
                
                ui.add_space(12.0);
                // Settings button (placeholder circle)
                let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 15.0, Colors::WHITE);
            });
        });

        ui.add_space(12.0);

        // Content area - white with rounded corners
        egui::Frame::default()
            .fill(Colors::WHITE)
            .corner_radius(36.0)
            .inner_margin(24.0)
            .show(ui, |ui| {
                match &state.screen {
                    Screen::Downloading { progress } => self.draw_download(ui, *progress),
                    Screen::Extracting { progress } => self.draw_extract(ui, *progress),
                    Screen::Starting => self.draw_starting(ui, rotation),
                    Screen::Ready { devices } => self.draw_ready(ui, devices),
                }
            });
    }

    /// Draw downloading screen
    fn draw_download(&self, ui: &mut egui::Ui, progress: f32) {
        ui.label(egui::RichText::new("Tải xuống trình biên dịch:").color(Colors::TEXT_DARK).size(24.0));
        ui.add_space(12.0);
        self.draw_progress_bar(ui, progress);
    }

    /// Draw extracting screen
    fn draw_extract(&self, ui: &mut egui::Ui, progress: f32) {
        ui.label(egui::RichText::new("Giải nén trình biên dịch:").color(Colors::TEXT_DARK).size(24.0));
        ui.add_space(12.0);
        self.draw_progress_bar(ui, progress);
    }

    /// Draw starting screen with spinner
    fn draw_starting(&self, ui: &mut egui::Ui, rotation: f32) {
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            self.draw_spinner(ui, rotation);
            ui.add_space(24.0);
            ui.label(egui::RichText::new("Đang kiểm tra CLI...").color(Colors::TEXT_DARK).size(24.0));
        });
    }

    /// Draw ready screen with device list
    fn draw_ready(&self, ui: &mut egui::Ui, devices: &[DeviceInfo]) {
        // Header with gradient
        let available = ui.available_width();

        // Gradient background
        let header_rect = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(available, 40.0)
        );
        ui.allocate_at_least(egui::vec2(available, 40.0), egui::Sense::hover());

        // Draw gradient as solid rectangle
        ui.painter().rect_filled(header_rect, 0.0, Colors::GRADIENT_START);

        // Header icon
        ui.painter().circle_filled(
            egui::Pos2::new(header_rect.min.x + 20.0, header_rect.center().y),
            12.0,
            Colors::WHITE
        );

        // Header text - use label instead of ui.put
        ui.label(egui::RichText::new("Danh sách thiết bị:").color(Colors::WHITE).size(20.0));

        ui.add_space(12.0);

        // Scrollable device list
        egui::ScrollArea::vertical().id_salt("device_list").show(ui, |ui| {
            egui::Frame::default()
                .fill(egui::Color32::from_rgb(243, 243, 243))
                .corner_radius(8.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    if devices.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(40.0);
                            ui.label(egui::RichText::new("Không có thiết bị nào được kết nối")
                                .color(Colors::TEXT_SECONDARY)
                                .size(16.0));
                            ui.add_space(40.0);
                        });
                    } else {
                        for device in devices {
                            self.draw_device_card(ui, device);
                            ui.add_space(8.0);
                        }
                    }
                });
        });

        ui.add_space(12.0);

        // Float button bar
        self.draw_float_bar(ui);
    }

    /// Draw a single device card
    fn draw_device_card(&self, ui: &mut egui::Ui, device: &DeviceInfo) {
        egui::Frame::default()
            .fill(Colors::WHITE)
            .stroke(egui::Stroke::new(1.0, Colors::CARD_BORDER))
            .corner_radius(24.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                // Device name with blue background
                ui.horizontal_wrapped(|ui| {
                    // Device icon
                    ui.painter().circle_filled(
                        egui::Pos2::new(ui.cursor().min.x + 20.0, ui.cursor().center().y),
                        10.0,
                        Colors::WHITE
                    );

                    ui.add_space(16.0);

                    // Device name
                    ui.label(egui::RichText::new(format!("{} ({})", device.name, device.port))
                        .color(Colors::WHITE)
                        .size(16.0));
                });

                ui.add_space(12.0);

                // Device info
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Port: ").color(Colors::TEXT_SECONDARY).size(16.0));
                    ui.label(egui::RichText::new(&device.port).color(Colors::TEXT_TERTIARY).size(16.0));

                    ui.add_space(24.0);

                    ui.label(egui::RichText::new("PID: ").color(Colors::TEXT_SECONDARY).size(16.0));
                    ui.label(egui::RichText::new(&device.pid).color(Colors::TEXT_TERTIARY).size(16.0));

                    ui.add_space(24.0);

                    ui.label(egui::RichText::new("VID: ").color(Colors::TEXT_SECONDARY).size(16.0));
                    ui.label(egui::RichText::new(&device.vid).color(Colors::TEXT_TERTIARY).size(16.0));
                });
            });
    }

    /// Draw progress bar
    fn draw_progress_bar(&self, ui: &mut egui::Ui, progress: f32) {
        let available = ui.available_width();

        ui.horizontal(|ui| {
            // Progress track
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(available - 50.0, 26.0),
                egui::Sense::hover()
            );

            // Draw track
            ui.painter().rect_filled(
                rect,
                100.0,
                Colors::PROGRESS_TRACK
            );

            // Draw fill
            let fill_width = (rect.width() - 4.0) * (progress / 100.0).min(1.0);
            if fill_width > 0.0 {
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        egui::Pos2::new(rect.min.x + 2.0, rect.min.y + 2.0),
                        egui::vec2(fill_width, 22.0)
                    ),
                    100.0,
                    Colors::PROGRESS
                );
            }

            // Percentage
            ui.label(egui::RichText::new(format!("{}%", progress as i32))
                .color(Colors::TEXT_DARK)
                .size(16.0));
        });
    }

    /// Draw spinner animation
    fn draw_spinner(&self, ui: &mut egui::Ui, rotation: f32) {
        let size = 62.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(size, size),
            egui::Sense::hover()
        );

        let center = rect.center();
        let radius = 26.0;
        let num_dots = 8;

        for i in 0..num_dots {
            let angle = rotation + (i as f32 * std::f32::consts::TAU / num_dots as f32);
            let dot_radius = 5.0;
            let x = center.x + radius * angle.cos();
            let y = center.y + radius * angle.sin();

            let alpha = ((i as f32 / num_dots as f32) * 255.0) as u8;
            let color = egui::Color32::from_rgba_unmultiplied(24, 144, 255, alpha);

            ui.painter().circle_filled(egui::Pos2::new(x, y), dot_radius, color);
        }
    }

    /// Draw floating action button bar
    fn draw_float_bar(&self, ui: &mut egui::Ui) {
        let available = ui.available_width();
        let btn_height = 57.0;
        let btn_width = (available - 16.0) / 3.0;

        egui::Frame::default()
            .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 230))
            .stroke(egui::Stroke::new(1.0, Colors::CARD_BORDER))
            .corner_radius(36.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                // Website button
                if ui.add_sized(
                    egui::vec2(btn_width, btn_height),
                    egui::Button::new(egui::RichText::new("Website").color(Colors::PRIMARY).size(12.0))
                        .stroke(egui::Stroke::new(1.0, Colors::PRIMARY))
                        .corner_radius(24.0)
                ).clicked() {
                    // TODO: Open website
                }

                ui.add_space(8.0);

                // Console button
                if ui.add_sized(
                    egui::vec2(btn_width, btn_height),
                    egui::Button::new(egui::RichText::new("Console").color(Colors::CONSOLE_BTN).size(12.0))
                        .stroke(egui::Stroke::new(1.0, Colors::CARD_SHADOW))
                        .fill(Colors::WHITE)
                        .corner_radius(24.0)
                ).clicked() {
                    // TODO: Open console
                }

                ui.add_space(8.0);

                // Refresh button
                if ui.add_sized(
                    egui::vec2(btn_width, btn_height),
                    egui::Button::new(egui::RichText::new("Refresh").color(Colors::PRIMARY).size(12.0))
                        .stroke(egui::Stroke::new(1.0, Colors::PRIMARY))
                        .corner_radius(24.0)
                ).clicked() {
                    // TODO: Refresh devices
                }
            });
    }
}

impl eframe::App for FutureAcademyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Update rotation for spinner animation
        let rotation = {
            let mut state = self.state.write().unwrap();
            let elapsed = state.last_update.elapsed().as_secs_f32();
            state.rotation = elapsed * 3.0;
            state.rotation
        };

        let state = self.state.read().unwrap().clone();

        // Request repaint for animations
        ctx.request_repaint_after(std::time::Duration::from_millis(50));

        // Draw the UI
        egui::CentralPanel::default()
            .frame(egui::Frame::default()
                .fill(Colors::PRIMARY)
                .inner_margin(12.0))
            .show(ctx, |ui| {
                self.draw_ui(ui, &state, rotation);
            });
    }
}
