#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if tapconductor_app_lib::handle_omr_export_callback() {
        return;
    }
    tapconductor_app_lib::run();
}
