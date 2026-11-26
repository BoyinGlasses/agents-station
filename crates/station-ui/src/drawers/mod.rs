pub mod accounts;
pub mod burn;
pub mod group_chat;
pub mod mac_sys;
pub mod markdown;
pub mod modals;
pub mod session;
pub mod vault;
pub mod whiteboard;

use crate::i18n::I18nStore;
use crate::state::{ActiveDrawer, UiState};
use bevy_egui::egui;

/// Dispatches the active drawer or modal, dropping transient GPU caches when closed.
pub fn draw_active_drawer(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if state.timelapse_recording {
        modals::draw_timelapse_overlay(ctx, state, i18n);
    }

    let drawer = state.active_drawer;
    if drawer != ActiveDrawer::GroupChat {
        group_chat::clear_preview_cache(ctx, state);
    }
    if drawer != ActiveDrawer::PhoneQr {
        modals::clear_phone_qr_cache(ctx);
    }

    match drawer {
        ActiveDrawer::None => {}
        ActiveDrawer::Session => session::draw_session_drawer(ctx, state, i18n),
        ActiveDrawer::GroupChat => group_chat::draw_group_chat_drawer(ctx, state, i18n),
        ActiveDrawer::Whiteboard => whiteboard::draw_whiteboard_modal(ctx, state, i18n),
        ActiveDrawer::Wrapped => modals::draw_wrapped_modal(ctx, state, i18n),
        ActiveDrawer::System => mac_sys::draw_mac_drawer(ctx, state, i18n),
        ActiveDrawer::Burn => burn::draw_burn_drawer(ctx, state, i18n),
        ActiveDrawer::Vault => vault::draw_vault_drawer(ctx, state, i18n),
        ActiveDrawer::Accounts => accounts::draw_accounts_drawer(ctx, state, i18n),
        ActiveDrawer::PhoneQr => modals::draw_phone_modal(ctx, state, i18n),
        ActiveDrawer::AddHarness => modals::draw_add_harness_modal(ctx, state, i18n),
        ActiveDrawer::Settings => modals::draw_settings_modal(ctx, state, i18n),
        ActiveDrawer::Timeline => modals::draw_timeline_modal(ctx, state, i18n),
    }
}
