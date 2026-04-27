//! egui-based editor for the OddVoices plugin.

use nih_plug::prelude::Editor;
use nih_plug_egui::egui;
use nih_plug_egui::{create_egui_editor, widgets, EguiState};
use std::path::PathBuf;
use std::sync::Arc;

use crate::plugin::{OddVoicesParams, SharedState};

/// Create the egui editor for the plugin.
pub fn create_editor(
    params: Arc<OddVoicesParams>,
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
        },
        move |egui_ctx, _setter, state| {
            // Update callback - called every frame
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                ui.heading("OddVoices");

                ui.add_space(10.0);

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

                ui.add_space(10.0);

                // ── Lyrics text input ──
                ui.label("Lyrics:");
                let text_edit = egui::TextEdit::multiline(&mut state.lyrics_buffer)
                    .desired_width(f32::INFINITY)
                    .desired_rows(5)
                    .hint_text("Enter lyrics here... (e.g. Hel-lo world)");
                let response = ui.add(text_edit);

                // Sync lyrics buffer to shared state when text changes
                if response.changed() {
                    *state.shared.lyrics.lock().unwrap() = state.lyrics_buffer.clone();
                }

                // Copy and Paste buttons using arboard for reliable clipboard access
                ui.horizontal(|ui| {
                    if ui.button("📋 Copy").clicked() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(state.lyrics_buffer.clone());
                        }
                    }
                    if ui.button("📄 Paste").clicked() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            if let Ok(text) = clipboard.get_text() {
                                state.lyrics_buffer = text;
                                *state.shared.lyrics.lock().unwrap() = state.lyrics_buffer.clone();
                            }
                        }
                    }
                });

                ui.add_space(10.0);

                // ── Gain slider ──
                ui.label("Gain");
                ui.add(widgets::ParamSlider::for_param(
                    &params.gain,
                    &_setter,
                ));

                ui.add_space(5.0);

                // ── Vibrato frequency slider ──
                ui.label("Vibrato Frequency");
                ui.add(widgets::ParamSlider::for_param(
                    &params.vibrato_frequency,
                    &_setter,
                ));

                // Vibrato depth slider
                ui.label("Vibrato Depth");
                ui.add(widgets::ParamSlider::for_param(
                    &params.vibrato_depth,
                    &_setter,
                ));

                ui.add_space(5.0);

                // ── Portamento time slider ──
                ui.label("Portamento Time");
                ui.add(widgets::ParamSlider::for_param(
                    &params.portamento_time,
                    &_setter,
                ));

                ui.add_space(10.0);

                // ── Reset button ──
                ui.horizontal(|ui| {
                    if ui.button("🔄 Reset Synth").clicked() {
                        // Set the atomic flag - the audio thread will check and clear it
                        state.shared.reset_requested.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    ui.label("(or automate the Reset param)");
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
