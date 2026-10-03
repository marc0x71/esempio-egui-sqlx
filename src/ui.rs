use eframe::egui::{self, Frame, RichText, Widget};
use modern_egui::theme::{
    self, StyledText, UiButtons, UiInputs, UiMetrics, UiPanels, UiText, metrics,
};

use crate::{app::TodoApp, db::DbBackend, model::Todo};

#[derive(Debug, PartialEq, Eq)]
enum TodoAction {
    Toggle(bool),
    Delete,
}

pub struct TodoUi<B: DbBackend> {
    app: TodoApp<B>,
    new_title: String,
    last_error: Option<String>,
}

impl<B: DbBackend> TodoUi<B> {
    pub fn new(cc: &eframe::CreationContext<'_>, app: TodoApp<B>) -> Self {
        theme::apply(&cc.egui_ctx);
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);

        Self {
            app,
            new_title: String::new(),
            last_error: None,
        }
    }
}

impl<B: DbBackend> eframe::App for TodoUi<B> {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let update = self.app.update();
        if let Some(error) = update.error {
            self.last_error = Some(error);
        }
        if update.changed {
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(Frame::new().inner_margin(metrics::PANEL_PADDING))
            .show(ui, |ui| {
                let p = theme::Palette::of(ui.ctx());
                ui.label(
                    RichText::new("TodoApp")
                        .color(p.text_strong)
                        .size(metrics::FONT_2XL)
                        .strong(),
                );

                ui.space3();

                ui.horizontal(|ui| {
                    let response =
                        ui.text_input_hint(&mut self.new_title, "What do we need to add?");

                    let enter_pressed =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    if ui.primary_button("Add").clicked() || enter_pressed {
                        self.app.add_todo(std::mem::take(&mut self.new_title));
                        response.request_focus();
                    }
                });

                ui.space_section();

                let mut dismiss = false;
                if let Some(error) = &self.last_error {
                    ui.horizontal(|ui| {
                        ui.add(
                            StyledText::new(format!("Error: {error}"))
                                .size(theme::text::TextSize::Lg)
                                .color(theme::text::TextColor::Danger),
                        );
                        dismiss = ui.button("❌").clicked();
                    });
                }
                if dismiss {
                    self.last_error = None;
                }

                ui.muted_label("TODOS");
                if !self.app.todos().is_empty() {
                    let available_height = ui.available_height();

                    egui::ScrollArea::vertical()
                        .max_height(available_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for (idx, todo) in self.app.todos().iter().enumerate() {
                                if idx > 0 {
                                    ui.space2();
                                }
                                ui.horizontal(|ui| {
                                    let mut widget = TodoWidget::new(todo);
                                    ui.add(&mut widget);
                                    if let Some(action) = widget.take_action() {
                                        match action {
                                            TodoAction::Toggle(done) => {
                                                self.app.set_todo_done(todo.id, done)
                                            }
                                            TodoAction::Delete => self.app.delete_todo(todo.id),
                                        }
                                    }
                                });
                            }
                        });
                }
            });
    }
}

pub struct TodoWidget<'a> {
    todo: &'a Todo,
    action: Option<TodoAction>,
}

impl<'a> TodoWidget<'a> {
    fn new(todo: &'a Todo) -> Self {
        Self { todo, action: None }
    }

    fn take_action(&mut self) -> Option<TodoAction> {
        self.action.take()
    }
}

impl Widget for &mut TodoWidget<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let mut done = self.todo.done;
        let p = theme::Palette::of(ui.ctx());
        ui.interactive_card(|ui| {
            if ui.checkbox(&mut done, "").changed() {
                self.action = Some(TodoAction::Toggle(done))
            }

            let title = if done {
                egui::RichText::new(&self.todo.title)
                    .color(p.danger)
                    .strikethrough()
            } else {
                egui::RichText::new(&self.todo.title).color(p.text_strong)
            };
            ui.label(title.size(metrics::FONT_CT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.danger_button("🗑").clicked() {
                    self.action = Some(TodoAction::Delete)
                }
            });
        })
        .response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    fn todo(done: bool) -> Todo {
        Todo {
            id: 1,
            title: "Buy milk".into(),
            done,
        }
    }

    #[test]
    fn todo_widget_starts_without_action() {
        let todo = todo(false);
        let mut widget = TodoWidget::new(&todo);
        assert!(widget.take_action().is_none());
    }

    #[test]
    fn clicking_checkbox_emits_toggle_true() {
        let todo = todo(false);

        let mut harness = Harness::new_ui_state(
            |ui, action: &mut Option<TodoAction>| {
                let mut widget = TodoWidget::new(&todo);

                ui.add(&mut widget);

                if let Some(new_action) = widget.take_action() {
                    *action = Some(new_action);
                }
            },
            None,
        );

        harness.get_by_role(egui::accesskit::Role::CheckBox).click();
        harness.run();

        assert_eq!(harness.state(), &Some(TodoAction::Toggle(true)));
    }

    #[test]
    fn clicking_checked_checkbox_emits_toggle_false() {
        let todo = todo(true);

        let mut harness = Harness::new_ui_state(
            |ui, action: &mut Option<TodoAction>| {
                let mut widget = TodoWidget::new(&todo);

                ui.add(&mut widget);

                if let Some(new_action) = widget.take_action() {
                    *action = Some(new_action);
                }
            },
            None,
        );

        harness.get_by_role(egui::accesskit::Role::CheckBox).click();
        harness.run();

        assert!(matches!(harness.state(), Some(TodoAction::Toggle(false))));
    }

    #[test]
    fn clicking_delete_emits_delete_action() {
        let todo = todo(false);

        let mut harness = Harness::new_ui_state(
            |ui, action: &mut Option<TodoAction>| {
                let mut widget = TodoWidget::new(&todo);

                ui.add(&mut widget);

                if let Some(new_action) = widget.take_action() {
                    *action = Some(new_action);
                }
            },
            None,
        );

        harness.get_by_label("🗑").click();
        harness.run();

        assert!(matches!(harness.state(), Some(TodoAction::Delete)));
    }
}
