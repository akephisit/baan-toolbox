mod bridge;
mod engine;
mod ui;
use wasm_bindgen::prelude::*;
#[wasm_bindgen(start)]
pub fn start() {
    dioxus::launch(ui::App);
}
