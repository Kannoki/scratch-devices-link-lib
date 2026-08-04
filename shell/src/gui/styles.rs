//! Design tokens and styling constants matching the Figma design.
//!
//! Color palette from Figma:
//! - Primary blue: #0e69b3
//! - Progress blue: #1890ff
//! - Progress track: #f5f5f5
//! - Card shadow blue: #0061af
//! - Header gradient: #f6c149 to #eb5759
//! - Console button yellow: #eeb462
//! - Text dark: #1e1e1e
//! - Text secondary: #525252
//! - Text tertiary: #404040
//! - Card border: #e5e5e5
//! - White: #ffffff

use eframe::egui;

/// Application color palette
pub struct Colors;

impl Colors {
    // Main colors
    pub const PRIMARY: egui::Color32 = egui::Color32::from_rgb(14, 105, 179);
    pub const WHITE: egui::Color32 = egui::Color32::from_rgb(255, 255, 255);

    // Progress bar
    pub const PROGRESS: egui::Color32 = egui::Color32::from_rgb(24, 144, 255);
    pub const PROGRESS_TRACK: egui::Color32 = egui::Color32::from_rgb(245, 245, 245);

    // Device card
    pub const CARD_BORDER: egui::Color32 = egui::Color32::from_rgb(229, 229, 229);
    pub const CARD_SHADOW: egui::Color32 = egui::Color32::from_rgb(0, 97, 175);

    // Header gradient colors (yellow to pink)
    pub const GRADIENT_START: egui::Color32 = egui::Color32::from_rgb(246, 193, 73);
    pub const GRADIENT_END: egui::Color32 = egui::Color32::from_rgb(235, 87, 97);

    // Console button
    pub const CONSOLE_BTN: egui::Color32 = egui::Color32::from_rgb(238, 180, 98);

    // Text colors
    pub const TEXT_DARK: egui::Color32 = egui::Color32::from_rgb(30, 30, 30);
    pub const TEXT_SECONDARY: egui::Color32 = egui::Color32::from_rgb(82, 82, 82);
    pub const TEXT_TERTIARY: egui::Color32 = egui::Color32::from_rgb(64, 64, 64);

    // Button colors
    pub const BTN_PRIMARY: egui::Color32 = egui::Color32::from_rgb(24, 144, 255);
    pub const BTN_SECONDARY_BORDER: egui::Color32 = egui::Color32::from_rgb(217, 217, 217);
}

/// Layout constants
pub struct Layout;

impl Layout {
    // Outer window padding
    pub const OUTER_PADDING: f32 = 12.0;
    // Inner content padding
    pub const INNER_PADDING: f32 = 24.0;
    // Outer corner radius
    pub const OUTER_RADIUS: f32 = 32.0;
    // Inner corner radius
    pub const INNER_RADIUS: f32 = 36.0;
    // Header gradient radius
    pub const GRADIENT_RADIUS: f32 = 20.0;
    // Card corner radius
    pub const CARD_RADIUS: f32 = 24.0;
    // Progress bar radius
    pub const PROGRESS_RADIUS: f32 = 100.0;
    // Float button radius
    pub const FLOAT_BTN_RADIUS: f32 = 24.0;
    // App corner radius
    pub const APP_RADIUS: f32 = 32.0;

    // Spacing
    pub const GAP: f32 = 12.0;
    pub const GAP_SMALL: f32 = 8.0;
    pub const GAP_LARGE: f32 = 24.0;

    // Header height
    pub const HEADER_HEIGHT: f32 = 60.0;
    // Logo size main
    pub const LOGO_SIZE_MAIN: f32 = 36.0;
    // Logo size splash
    pub const LOGO_SIZE_SPLASH: f32 = 54.0;
    // Icon size
    pub const ICON_SIZE: f32 = 24.0;
    // Close button size
    pub const CLOSE_BTN_SIZE: f32 = 36.0;

    // Content heights
    pub const PROGRESS_CONTENT_HEIGHT: f32 = 106.0;
    pub const MAIN_CONTENT_HEIGHT: f32 = 511.0;
    pub const FLOAT_HEIGHT: f32 = 57.0;
}

/// Font families (mapped from Figma Quicksand/DM Sans)
pub struct Fonts;

impl Fonts {
    pub const TITLE_FONT: &'static str = "Quicksand Bold";
    pub const BODY_FONT: &'static str = "DM Sans";
}
