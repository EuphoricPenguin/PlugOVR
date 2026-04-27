//! egui-based editor for the OddVoices plugin.

use nih_plug::prelude::Editor;
use nih_plug_egui::egui;
use nih_plug_egui::{create_egui_editor, EguiState};
use std::path::PathBuf;
use std::sync::Arc;

use crate::plugin::{OddVoicesParams, SharedState};

/// Create the egui editor for the plugin.
pub fn create_editor(
    _params: Arc<OddVoicesParams>,
    editor_state: Arc<EguiState>,
    shared: Arc<SharedState>,
) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        editor_state,
        EguiEditorState {
            shared,
            lyrics_buffer: String::new(),
            selected_voice: String::new(),
        },
        |_egui_ctx, state| {
            // Build callback - initialize buffers from shared state
            state.lyrics_buffer = state.shared.lyrics.lock().unwrap().clone();
            state.selected_voice = state.shared.current_voice.lock().unwrap().clone();

            // Default to "cicada" if no voice is selected and it's available
            if state.selected_voice.is_empty() {
                let available = state.shared.available_voices.lock().unwrap().clone();
                if available.contains(&"cicada".to_string()) {
                    state.selected_voice = "cicada".to_string();
                    let voice_path = resolve_voice_path("cicada");
                    *state.shared.pending_voice.lock().unwrap() = Some(voice_path);
                    *state.shared.current_voice.lock().unwrap() = "cicada".to_string();
                }
            }
        },
        move |egui_ctx, _setter, state| {
            // Update callback - called every frame
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 4)))


                .show(egui_ctx, |ui| {
                    ui.heading("PlugOVR");

                    // ── Voice selection dropdown ──
                    ui.label("Voice:");
                    let available = state.shared.available_voices.lock().unwrap().clone();
                    let current = state.selected_voice.clone();

                    egui::ComboBox::from_id_salt("voice_selector")
                        .selected_text(if current.is_empty() {
                            "Select a voice...".to_string()
                        } else {
                            current.clone()
                        })
                        .show_ui(ui, |ui| {
                            for voice_name in &available {
                                let selected = voice_name == &current;
                                if ui.selectable_label(selected, voice_name).clicked() {
                                    state.selected_voice = voice_name.clone();
                                    // Set the pending voice path for the audio thread
                                    let voice_path = resolve_voice_path(voice_name);
                                    *state.shared.pending_voice.lock().unwrap() = Some(voice_path);
                                    *state.shared.current_voice.lock().unwrap() = voice_name.clone();
                                }
                            }
                        });

                    // ── Lyrics text input (fixed size with scrollbar) ──
                    ui.label("Lyrics:");
                    let text_edit = egui::TextEdit::multiline(&mut state.lyrics_buffer)
                        .desired_width(f32::INFINITY)
                        .desired_rows(8)
                        .hint_text("Use the paste button to quickly input lyrics from your clipboard.");
                    let response = egui::ScrollArea::vertical()
                        .id_salt("lyrics_scroll")
                        .max_height(160.0)
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            ui.add(text_edit)
                        });
                    let response = response.inner;

                    // Sync lyrics buffer to shared state when text changes
                    if response.changed() {
                        *state.shared.lyrics.lock().unwrap() = state.lyrics_buffer.clone();
                    }

                    // Paste and Clear buttons
                    ui.horizontal(|ui| {
                        if ui.button("📄 Paste").clicked() {
                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                if let Ok(text) = clipboard.get_text() {
                                    state.lyrics_buffer = text;
                                    *state.shared.lyrics.lock().unwrap() = state.lyrics_buffer.clone();
                                }
                            }
                        }
                        if ui.button("🗑 Clear").clicked() {
                            state.lyrics_buffer.clear();
                            *state.shared.lyrics.lock().unwrap() = state.lyrics_buffer.clone();
                        }
                    });
                });
        },
    )
}

/// Resolve a voice name to its full file path.
fn resolve_voice_path(voice_name: &str) -> PathBuf {
    // Try the cargo manifest directory first (for development)
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev_path = manifest_dir
        .join("bin")
        .join("compiled_voices")
        .join(format!("{}.voice", voice_name));
    if dev_path.exists() {
        return dev_path;
    }

    // Fall back to relative to the executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_path = exe_dir
                .join("compiled_voices")
                .join(format!("{}.voice", voice_name));
            if exe_path.exists() {
                return exe_path;
            }
        }
    }

    // Last resort: just return the dev path anyway
    dev_path
}

/// State for the egui editor.
struct EguiEditorState {
    /// Shared state with the audio thread.
    shared: Arc<SharedState>,
    /// Local buffer for the text edit widget.
    lyrics_buffer: String,
    /// Currently selected voice name.
    selected_voice: String,
}
