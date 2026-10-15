//! First-run wizard screens. A child of `settings_ui` so it can reuse the
//! settings window's private widgets (`perm_row`, `pane_heading`, …).

use super::*;
use crate::onboarding::{self, Platform, Probe, Step};

pub struct OnboardingView<'a> {
    /// `None` is the 完成 screen.
    pub step: Option<Step>,
    pub platform: Platform,
    pub probe: Probe,
    pub stt_warning: &'a str,
    /// Only text recorded after the trial step began.
    pub last_final: &'a str,
    pub status: &'a str,
    pub hotkey_label: &'a str,
    /// The global hotkey hook is running.
    pub hotkey_live: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnboardingAction {
    None,
    Next,
    Later,
    Download,
    UseModel(String),
    Relaunch,
    Finish,
}

fn primary_button(ui: &mut Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(label).color(Color32::WHITE))
            .fill(theme::GLASS_TINT)
            .min_size(Vec2::new(0.0, ROW_H)),
    )
}

impl SettingsForm {
    pub fn show_onboarding(
        &mut self,
        ctx: &egui::Context,
        view: &OnboardingView<'_>,
    ) -> OnboardingAction {
        let mut action = OnboardingAction::None;
        paint_settings_titlebar(ctx);
        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::BG).inner_margin(0))
            .show(ctx, |ui| {
                theme::content_pane_frame().show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("lailaisay_onboarding")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            action = match view.step {
                                Some(Step::Welcome) => self.onboarding_welcome(ui, view),
                                Some(Step::Model) => self.onboarding_model(ui, view),
                                Some(Step::Permissions) => self.onboarding_permissions(ui, view),
                                Some(Step::Trial) => self.onboarding_trial(ui, view),
                                None => self.onboarding_done(ui),
                            };
                            if view.step.is_some() {
                                ui.add_space(16.0);
                                if ghost_button(ui, "稍後再說").clicked() {
                                    action = OnboardingAction::Later;
                                }
                                note(ui, "未完成前，下次開啟 lailaisay 仍會回到這個引導。");
                            }
                        });
                });
            });
        crate::status_hud::paint_recording_hud(ctx, view.status);
        action
    }

    fn onboarding_welcome(&mut self, ui: &mut Ui, view: &OnboardingView<'_>) -> OnboardingAction {
        pane_heading(
            ui,
            "歡迎使用 lailaisay",
            "按住快捷鍵說話，放開後文字會貼到目前的輸入位置。開始前需要完成幾個設定。",
        );
        note(ui, &format!("快捷鍵：{}", view.hotkey_label));
        ui.add_space(8.0);
        note(ui, "步驟：選擇語音模型 → 開啟系統權限 → 說一句話試用。");
        ui.add_space(12.0);
        if primary_button(ui, "開始設定", true).clicked() {
            return OnboardingAction::Next;
        }
        OnboardingAction::None
    }

    fn onboarding_model(&mut self, ui: &mut Ui, view: &OnboardingView<'_>) -> OnboardingAction {
        let mut action = OnboardingAction::None;
        pane_heading(
            ui,
            "選擇語音模型",
            "語音在本機辨識，模型需要先下載一次，之後不用網路。",
        );
        for id in [onboarding::recommended_model_id(), onboarding::SMALL_MODEL_ID] {
            let Some(entry) = catalog_by_id(id) else {
                continue;
            };
            if ui
                .selectable_label(self.selected_catalog_id == entry.id, catalog_row_label(entry))
                .clicked()
            {
                self.selected_catalog_id = entry.id.to_string();
            }
        }
        ui.add_space(8.0);
        let can_download = !self.downloading && catalog_by_id(&self.selected_catalog_id).is_some();
        let label = if self.downloading { "下載中…" } else { "下載並使用" };
        if primary_button(ui, label, can_download).clicked() {
            action = OnboardingAction::Download;
        }
        if !self.download_message.is_empty() {
            note(ui, &self.download_message);
        }
        if !view.stt_warning.is_empty() {
            danger_note(ui, view.stt_warning);
        }
        if !self.downloading && !self.selected_model.is_empty() && !view.probe.model_ready {
            note(ui, "模型載入中，通常需要數秒到十幾秒…");
            ui.ctx().request_repaint_after(Duration::from_millis(500));
        }

        if !self.local_models.is_empty() {
            ui.add_space(12.0);
            note(ui, "沒有網路？可以直接使用電腦上已有的模型：");
            for model in self.local_models.iter().take(4) {
                let text = format!("使用已有的模型：{}", friendly_model_label(model));
                if ghost_button(ui, &text).clicked() {
                    action = OnboardingAction::UseModel(model.path.to_string_lossy().into_owned());
                }
            }
        }
        action
    }

    fn onboarding_permissions(
        &mut self,
        ui: &mut Ui,
        view: &OnboardingView<'_>,
    ) -> OnboardingAction {
        // Re-probe while on screen so System Settings toggles show up.
        ui.ctx().request_repaint_after(Duration::from_millis(500));
        pane_heading(
            ui,
            "開啟系統權限",
            "按下每一列的「開啟」，在系統設定中允許 lailaisay。授權後這裡會自動更新。",
        );
        for pane in view.platform.required_permissions() {
            let (title, detail) = PRIVACY_ROWS
                .iter()
                .find(|(p, _, _)| p == pane)
                .map(|(_, t, d)| (*t, *d))
                .unwrap_or(("權限", ""));
            if let Some(err) = perm_row(ui, title, detail, *pane) {
                self.permissions_open_message = err;
            }
        }
        if !self.permissions_open_message.is_empty() {
            danger_note(ui, &self.permissions_open_message);
        }
        let mut action = OnboardingAction::None;
        let hooks_granted = view.probe.accessibility == GrantStatus::Granted
            && view.probe.input_monitoring == GrantStatus::Granted;
        if view.platform == Platform::MacDeveloperId && hooks_granted && !view.hotkey_live {
            ui.add_space(8.0);
            note(ui, "權限已開啟，但快捷鍵需要重新啟動 lailaisay 才會生效。");
            if primary_button(ui, "重新啟動 lailaisay", true).clicked() {
                action = OnboardingAction::Relaunch;
            }
        }
        action
    }

    fn onboarding_trial(&mut self, ui: &mut Ui, view: &OnboardingView<'_>) -> OnboardingAction {
        pane_heading(
            ui,
            "試用一下",
            &format!(
                "按住 {} 說一句話，再放開。下方出現文字就完成了。",
                view.hotkey_label
            ),
        );
        paint_status_strip(ui, view.status, "");
        ui.add_space(8.0);
        show_readonly_block(ui, view.last_final, "辨識結果會顯示在這裡");
        let mut action = OnboardingAction::None;
        if !view.hotkey_live {
            danger_note(ui, "快捷鍵尚未啟用，請先確認系統權限，必要時重新啟動 lailaisay。");
            if view.platform == Platform::MacDeveloperId
                && primary_button(ui, "重新啟動 lailaisay", true).clicked()
            {
                action = OnboardingAction::Relaunch;
            }
        } else if view.platform == Platform::Windows {
            note(ui, "若沒有反應，請確認 Windows 設定中麥克風已允許桌面應用程式使用。");
        }
        action
    }

    fn onboarding_done(&mut self, ui: &mut Ui) -> OnboardingAction {
        pane_heading(
            ui,
            "設定完成",
            "可以開始使用了。lailaisay 會留在選單列（Windows 在通知區），按住快捷鍵就能說話。",
        );
        if primary_button(ui, "開始使用", true).clicked() {
            return OnboardingAction::Finish;
        }
        OnboardingAction::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view<'a>(step: Option<Step>, probe: Probe) -> OnboardingView<'a> {
        OnboardingView {
            step,
            platform: Platform::MacDeveloperId,
            probe,
            stt_warning: "",
            last_final: "",
            status: "idle",
            hotkey_label: "⌘⇧Space",
            hotkey_live: true,
        }
    }

    fn denied() -> Probe {
        Probe {
            model_ready: false,
            microphone: GrantStatus::Denied,
            accessibility: GrantStatus::Denied,
            input_monitoring: GrantStatus::Denied,
        }
    }

    fn paint(
        ctx: &egui::Context,
        form: &mut SettingsForm,
        view: &OnboardingView<'_>,
    ) -> OnboardingAction {
        let size = Vec2::new(SETTINGS_INNER_SIZE[0], SETTINGS_INNER_SIZE[1]);
        let mut action = OnboardingAction::None;
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                action = form.show_onboarding(ctx, view);
            });
        }
        action
    }

    #[test]
    fn every_step_paints_its_heading_below_the_titlebar_and_inside_the_window() {
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        let steps = [
            (Some(Step::Welcome), "歡迎使用 lailaisay"),
            (Some(Step::Model), "選擇語音模型"),
            (Some(Step::Permissions), "開啟系統權限"),
            (Some(Step::Trial), "試用一下"),
            (None, "設定完成"),
        ];
        for (step, heading) in steps {
            let v = view(step, denied());
            let action = paint(&ctx, &mut form, &v);
            assert_eq!(action, OnboardingAction::None, "{heading}: no click, no action");
            let resp = ctx
                .read_response(pane_heading_id(heading))
                .unwrap_or_else(|| panic!("`{heading}` heading should paint"));
            assert!(
                resp.rect.top() >= SETTINGS_TITLEBAR_H - 1.0,
                "{heading} top {} sits under the titlebar",
                resp.rect.top()
            );
            assert!(
                resp.rect.bottom() <= SETTINGS_INNER_SIZE[1],
                "{heading} falls off the window"
            );
        }
    }

    #[test]
    fn model_step_keeps_the_failure_message_and_offers_local_models() {
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        form.download_message = "Download failed: network".into();
        form.local_models = vec![crate::models::FoundModel {
            path: "/tmp/ggml-tiny.bin".into(),
            source: crate::models::ModelSource::Local,
        }];
        let v = view(Some(Step::Model), denied());
        let _ = paint(&ctx, &mut form, &v);
        assert!(form.download_message.contains("failed"));
        assert_eq!(form.local_models.len(), 1);
    }

    #[test]
    fn caller_preselects_the_recommended_card_and_painting_keeps_it() {
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        form.selected_catalog_id = onboarding::recommended_model_id().into();
        let v = view(Some(Step::Model), denied());
        let _ = paint(&ctx, &mut form, &v);
        assert_eq!(form.selected_catalog_id, onboarding::recommended_model_id());
    }
}
