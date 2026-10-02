use eframe::egui::{self, Frame, RichText};
use modern_egui::theme::{
    self, UiButtons, UiInputs, UiMetrics, UiPanels, UiText, metrics, text::StyledText,
};

use crate::{
    db::{DbCommand, DbEvent, DbHandle},
    model::Todo,
};

pub struct TodoApp {
    db: DbHandle,

    todos: Vec<Todo>,
    new_title: String,

    error: Option<String>,
}

impl TodoApp {
    pub fn new(cc: &eframe::CreationContext<'_>, db: DbHandle) -> Self {
        theme::apply(&cc.egui_ctx);
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);

        let mut app = Self {
            db,
            todos: Vec::new(),
            new_title: String::new(),
            error: None,
        };

        app.load_todos();

        app
    }

    fn load_todos(&mut self) {
        let _ = self.db.send(DbCommand::LoadTodos);
    }

    fn add_todo(&mut self) {
        let title = self.new_title.trim();

        if title.is_empty() {
            return;
        }

        if self
            .db
            .send(DbCommand::AddTodo {
                title: title.to_owned(),
            })
            .is_ok()
        {
            self.new_title.clear();
        }
    }

    fn handle_db_results(&mut self) -> bool {
        let mut changed = false;
        while let Some(event) = self.db.try_recv() {
            match event {
                DbEvent::TodosLoaded(todos) => {
                    self.todos = todos;
                    self.error = None;
                    changed = true;
                }
                DbEvent::TodoAdded | DbEvent::TodoUpdated | DbEvent::TodoDeleted => {
                    self.load_todos();
                }
                DbEvent::Error(err) => {
                    self.error = Some(err);
                    changed = true;
                }
            }
        }
        changed
    }
}

impl eframe::App for TodoApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.handle_db_results() {
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
                        self.add_todo();
                        response.request_focus();
                    }
                });

                ui.space_section();

                let mut dismiss = false;
                if let Some(error) = &self.error {
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
                    self.error = None;
                }

                ui.muted_label("TODOS");
                if !self.todos.is_empty() {
                    let available_height = ui.available_height();

                    egui::ScrollArea::vertical()
                        .max_height(available_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for (idx, todo) in self.todos.iter().enumerate() {
                                if idx > 0 {
                                    ui.space2();
                                }
                                ui.horizontal(|ui| {
                                    if let Some(action) = todo_row(ui, todo) {
                                        match action {
                                            TodoAction::Toggle(done) => {
                                                let _ = self.db.send(DbCommand::SetTodoDone {
                                                    id: todo.id,
                                                    done,
                                                });
                                            }
                                            TodoAction::Delete => {
                                                let _ = self
                                                    .db
                                                    .send(DbCommand::DeleteTodo { id: todo.id });
                                            }
                                        }
                                    }
                                });
                            }
                        });
                }
            });
    }
}

#[derive(Debug)]
enum TodoAction {
    Toggle(bool),
    Delete,
}

fn todo_row(ui: &mut egui::Ui, todo: &Todo) -> Option<TodoAction> {
    let mut action = None;

    let p = theme::Palette::of(ui.ctx());
    let mut done = todo.done;
    ui.interactive_card(|ui| {
        if ui.checkbox(&mut done, "").changed() {
            action = Some(TodoAction::Toggle(done));
        }

        let title = if done {
            egui::RichText::new(&todo.title)
                .color(p.danger)
                .strikethrough()
        } else {
            egui::RichText::new(&todo.title).color(p.text_strong)
        };
        ui.label(title.size(metrics::FONT_CT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.danger_button("🗑").clicked() {
                action = Some(TodoAction::Delete);
            }
        });
    });

    action
}
