//! The compact shell: one command bar, one context strip, one status line.
//!
//! The document workspace keeps the same rendering and editing code as the
//! original shell; only chrome, hierarchy and spacing are redesigned. Every
//! action of the old toolbar is still reachable: frequently used commands are
//! one click away, everything else is grouped in `File`, `Edit` and `View`.

use super::{docx_preview_plan, sanitize_view_zoom, NotOfficeApp};
use crate::ui_theme as theme;
use crate::ui_theme::Icon;
use eframe::egui;
use egui::{Align, Color32, Layout, RichText, Sense, Stroke, Vec2};
use liteoffice::text_document::TextKind;
use liteoffice::{tfm, tr};

fn menu_title(text: &str) -> RichText {
    RichText::new(text).size(12.5).color(theme::ink())
}

fn faint_chip_bg() -> Color32 {
    theme::soft_bg()
}

/// Menu-bar buttons are plain text until hovered; egui's default framed
/// button would give the whole menu bar a boxed, dated look.
fn flat_menu<R>(ui: &mut egui::Ui, title: &str, contents: impl FnOnce(&mut egui::Ui) -> R) {
    ui.scope(|ui| {
        let widgets = &mut ui.style_mut().visuals.widgets;
        widgets.inactive.bg_stroke = Stroke::NONE;
        widgets.hovered.bg_stroke = Stroke::NONE;
        widgets.active.bg_stroke = Stroke::NONE;
        widgets.open.bg_stroke = Stroke::NONE;
        ui.menu_button(menu_title(title), contents);
    });
}

impl NotOfficeApp {
    // -- visibility and small predicates -----------------------------------

    pub(crate) fn search_visible(&self) -> bool {
        self.search_active || self.search_focus_requested || !self.search_query.is_empty()
    }

    /// Turn the start screen into an editable, untitled text document without
    /// spawning a second window.
    fn start_blank_text(&mut self) {
        self.text = Some(Default::default());
        self.text_focus_pending = true;
        self.docx_editor_generation = self.docx_editor_generation.wrapping_add(1);
        self.status = tr!(
            "Новый текстовый документ; «Сохранить как» предлагает TXT, MD или простой RTF.",
            "New text document; Save As chooses TXT, MD or basic RTF."
        )
        .into();
    }

    fn can_save_document(&self) -> bool {
        self.text.is_some()
            || self.docx.is_some()
            || self
                .spreadsheet
                .as_ref()
                .is_some_and(|book| book.editor.is_some())
    }

    fn document_badge(&self) -> (&'static str, Color32, Color32) {
        let editing_workbook = self
            .spreadsheet
            .as_ref()
            .is_some_and(|book| book.editor.is_some());
        if self.pdf.is_some() {
            ("PDF", theme::muted(), faint_chip_bg())
        } else if let Some(document) = &self.text {
            let label = match document.kind {
                TextKind::Txt => "TXT",
                TextKind::Markdown => "MD",
                TextKind::Rtf => "RTF",
            };
            (label, theme::accent_ink(), theme::accent_soft())
        } else if self.spreadsheet.is_some() {
            if editing_workbook {
                (
                    tr!("XLSX · ПРАВКА", "XLSX · EDIT"),
                    theme::accent_ink(),
                    theme::accent_soft(),
                )
            } else {
                let label = self
                    .opened_path
                    .as_ref()
                    .and_then(|path| path.extension())
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.to_ascii_lowercase());
                let label = match label.as_deref() {
                    Some("csv" | "tsv") => "CSV",
                    Some("ods") => "ODS",
                    Some("xlsb") => "XLSB",
                    Some("xls") => "XLS",
                    _ => "XLSX",
                };
                (label, theme::muted(), faint_chip_bg())
            }
        } else if self.legacy_doc.is_some() {
            ("DOC", theme::warn(), theme::warn_soft())
        } else if let Some(package) = &self.docx {
            if package.requires_save_as() {
                ("DOC → DOCX", theme::warn(), theme::warn_soft())
            } else {
                ("DOCX", theme::accent_ink(), theme::accent_soft())
            }
        } else {
            (tr!("ФАЙЛ", "FILE"), theme::muted(), faint_chip_bg())
        }
    }

    fn document_title(&self) -> (String, String) {
        match &self.opened_path {
            Some(path) => {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                (name, path.display().to_string())
            }
            None => (
                tr!("Без имени", "Untitled").into(),
                tr!("Ещё не сохранён", "Not saved yet").into(),
            ),
        }
    }

    // -- command bar -------------------------------------------------------

    pub(crate) fn shell_command_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar")
            .exact_height(theme::BAR_HEIGHT)
            .frame(theme::bar_frame())
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if self.decision_pending() || self.save_prompt_open || self.legacy_edit_prompt {
                        ui.disable();
                    }
                    theme::brand(ui);
                    ui.add_space(2.0);
                    self.file_menu(ui);
                    self.edit_menu(ui);
                    self.view_menu(ui);
                    ui.add_space(2.0);
                    theme::hairline(ui);
                    ui.add_space(2.0);

                    if ui
                        .add_enabled(
                            self.can_save_document(),
                            theme::IconButton::new(
                                Icon::Save,
                                tr!("Сохранить (Ctrl+S)", "Save (Ctrl+S)"),
                            ),
                        )
                        .clicked()
                    {
                        self.request_save();
                    }
                    if ui
                        .add(theme::IconButton::new(
                            Icon::Folder,
                            tr!("Открыть… (Ctrl+O)", "Open… (Ctrl+O)"),
                        ))
                        .clicked()
                    {
                        self.open_document();
                    }
                    theme::hairline(ui);
                    let undo_tip = if self.can_undo() {
                        tr!(
                            "Отменить последнее действие (Ctrl+Z)",
                            "Undo last action (Ctrl+Z)"
                        )
                    } else {
                        tr!("Нет действий для отмены", "Nothing to undo")
                    };
                    if ui
                        .add_enabled(
                            self.can_undo(),
                            theme::IconButton::new(Icon::Undo, undo_tip),
                        )
                        .clicked()
                    {
                        self.undo();
                    }
                    let redo_tip = if self.can_redo() {
                        tr!(
                            "Вернуть отменённое действие (Ctrl+Shift+Z)",
                            "Redo undone action (Ctrl+Shift+Z)"
                        )
                    } else {
                        tr!(
                            "Нет отменённых действий: сначала нажмите «Отменить»",
                            "No undone actions: press Undo first"
                        )
                    };
                    if ui
                        .add_enabled(
                            self.can_redo(),
                            theme::IconButton::new(Icon::Redo, redo_tip),
                        )
                        .clicked()
                    {
                        self.redo();
                    }
                    let print_tip = if self.print_receiver.is_some() {
                        tr!(
                            "Печать уже идёт: закройте системный диалог или дождитесь завершения",
                            "Printing is already active: close the system dialog or wait"
                        )
                    } else {
                        tr!("Печать… (Ctrl+P)", "Print… (Ctrl+P)")
                    };
                    if ui
                        .add_enabled(
                            self.has_document(),
                            theme::IconButton::new(Icon::Printer, print_tip),
                        )
                        .clicked()
                    {
                        self.print_document();
                    }
                    self.recent_menu_button(ui);
                    if ui
                        .add_enabled(
                            self.docx.is_some()
                                || self.legacy_doc.is_some()
                                || self.spreadsheet.is_some(),
                            egui::Button::new(
                                RichText::new(tr!(
                                    "Сохранить очищенную копию",
                                    "Save clean copy"
                                ))
                                .size(11.0),
                            ),
                        )
                        .on_hover_text(tr!(
                            "Создать отдельную копию без макросов, OLE/ActiveX, вложений и внешних связей",
                            "Create a separate copy without macros, OLE/ActiveX, attachments or external links"
                        ))
                        .on_disabled_hover_text(tr!(
                            "Доступно для открытого DOC/DOCX или XLS/XLSX",
                            "Available for an open DOC/DOCX or XLS/XLSX"
                        ))
                        .clicked()
                    {
                        self.create_clean_copy();
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .selectable_label(liteoffice::i18n::is_ru(), "RU")
                            .on_hover_text(tr!("Русский интерфейс", "Russian interface"))
                            .clicked()
                        {
                            liteoffice::i18n::set_english(false);
                            let _ = liteoffice::i18n::save();
                        }
                        if ui
                            .selectable_label(!liteoffice::i18n::is_ru(), "EN")
                            .on_hover_text(tr!("English interface", "English interface"))
                            .clicked()
                        {
                            liteoffice::i18n::set_english(true);
                            let _ = liteoffice::i18n::save();
                        }
                        ui.add_space(4.0);
                        theme::hairline(ui);
                        ui.add_space(4.0);
                        if ui
                            .add(
                                theme::IconButton::new(
                                    Icon::Settings,
                                    tr!("Параметры печати", "Print setup"),
                                )
                                .selected(self.page_setup_open),
                            )
                            .clicked()
                        {
                            self.page_setup_open = !self.page_setup_open;
                        }
                        ui.add_space(2.0);
                        theme::hairline(ui);
                        ui.add_space(2.0);

                        if ui
                            .add(
                                theme::IconButton::new(
                                    Icon::Plus,
                                    tr!("Увеличить масштаб", "Zoom in"),
                                )
                                .small(),
                            )
                            .clicked()
                        {
                            self.zoom = sanitize_view_zoom(self.zoom + 0.25);
                        }
                        let mut percent = (self.zoom * 100.0).round() as u32;
                        let zoom_input = ui
                            .add_sized(
                                [54.0, 22.0],
                                egui::DragValue::new(&mut percent)
                                    .range(50..=400)
                                    .speed(5)
                                    .suffix("%"),
                            )
                            .on_hover_text(tr!(
                                "Перетащите для масштаба; двойной щелчок — ввод значения",
                                "Drag to zoom; double-click to type a value"
                            ));
                        if zoom_input.changed() {
                            self.zoom = sanitize_view_zoom(percent as f32 / 100.0);
                        }
                        if ui
                            .add(
                                theme::IconButton::new(
                                    Icon::Minus,
                                    tr!("Уменьшить масштаб", "Zoom out"),
                                )
                                .small(),
                            )
                            .clicked()
                        {
                            self.zoom = sanitize_view_zoom(self.zoom - 0.25);
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(tr!("Масштаб", "Scale")).size(11.0),
                                )
                                .frame(false),
                            )
                            .on_hover_text(tr!("Сбросить масштаб на 100%", "Reset zoom to 100%"))
                            .clicked()
                        {
                            self.zoom = 1.0;
                        }
                        self.zoom = sanitize_view_zoom(self.zoom);
                    });
                });
                theme::bottom_rule(ui);
            });
        self.shell_save_prompt(ctx);
        self.shell_legacy_edit_prompt(ctx);
        self.shell_page_setup(ctx);
    }

    fn shell_save_prompt(&mut self, ctx: &egui::Context) {
        if !self.save_prompt_open {
            return;
        }
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new(tr!("Сохранить копию", "Save a copy"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.label(&self.save_prompt_reason);
                ui.add_space(6.0);
                ui.label(
                    RichText::new(tr!(
                        "После выбора файла изменения будут записаны только в него. Исходный файл останется прежним.",
                        "After choosing a file, edits will be written only there. The source file stays unchanged."
                    ))
                    .size(11.0)
                    .color(theme::muted()),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(theme::PrimaryButton::new(tr!("Выбрать копию…", "Choose copy…")))
                        .clicked()
                    {
                        confirm = true;
                    }
                    if ui.button(tr!("Отмена", "Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        if confirm {
            self.save_prompt_open = false;
            self.save_document_as(self.save_prompt_copy);
        } else if cancel {
            self.save_prompt_open = false;
            self.status = tr!(
                "Сохранение отменено: правки остались в документе",
                "Save cancelled: edits stay in the document"
            )
            .into();
        }
    }

    fn shell_legacy_edit_prompt(&mut self, ctx: &egui::Context) {
        if !self.legacy_edit_prompt {
            return;
        }
        if self.legacy_doc.is_none() {
            self.legacy_edit_prompt = false;
            return;
        }
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new(tr!("Редактировать DOC?", "Edit DOC?"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(tr!(
                    "DOC открыт только для просмотра. Чтобы редактировать, NotOffice создаст новый DOCX из восстановленного текста.",
                    "DOC is view-only. To edit, NotOffice will create a new DOCX from the recovered text."
                ));
                ui.add_space(6.0);
                ui.label(
                    RichText::new(tr!(
                        "Оригинальный .doc не изменится. Стили, таблицы, колонки и точная вёрстка Word могут быть потеряны.",
                        "The original .doc stays unchanged. Styles, tables, columns and exact Word layout may be lost."
                    ))
                    .size(11.0)
                    .color(theme::muted()),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(theme::PrimaryButton::new(tr!(
                            "Создать DOCX и редактировать",
                            "Create DOCX and edit"
                        )))
                        .clicked()
                    {
                        confirm = true;
                    }
                    if ui
                        .button(tr!("Остаться в просмотре", "Keep viewing"))
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        if confirm {
            self.legacy_edit_prompt = false;
            self.edit_legacy_as_docx();
        } else if cancel {
            self.legacy_edit_prompt = false;
        }
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        flat_menu(ui, tr!("Файл", "File"), |ui| {
            ui.set_min_width(230.0);
            if ui
                .add(
                    egui::Button::new(tr!("Новый документ", "New document"))
                        .shortcut_text("Ctrl+N"),
                )
                .clicked()
            {
                self.open_new_blank_window();
                ui.close_menu();
            }
            ui.separator();
            if ui
                .add(egui::Button::new(tr!("Открыть…", "Open…")).shortcut_text("Ctrl+O"))
                .clicked()
            {
                self.open_document();
                ui.close_menu();
            }
            if ui
                .button(tr!("Открыть в новом окне…", "Open in new window…"))
                .clicked()
            {
                self.open_document_in_new_window();
                ui.close_menu();
            }
            if ui
                .button(tr!("Открыть с помощью…", "Open with…"))
                .on_hover_text(tr!(
                    "Выбрать Word, NotOffice или другую программу в окне Windows",
                    "Choose Word, NotOffice or another app in the Windows dialog"
                ))
                .clicked()
            {
                self.open_with_dialog();
                ui.close_menu();
            }
            let recents = self.recent_paths.clone();
            ui.separator();
            ui.menu_button(tr!("Недавние", "Open recent"), |ui| {
                self.recent_menu_contents(ui, &recents);
            });
            ui.separator();
            if ui
                .add_enabled(
                    self.can_save_document(),
                    egui::Button::new(tr!("Сохранить как…", "Save As…"))
                        .shortcut_text("Ctrl+Shift+S"),
                )
                .clicked()
            {
                self.save_document_as(false);
                ui.close_menu();
            }
            if ui
                .add_enabled(
                    self.legacy_doc.is_some(),
                    egui::Button::new(tr!("Сохранить разметку DOC…", "Save DOC layout…")),
                )
                .on_hover_text(
                    tr!("Сохранить позиции и ширину изображений рядом с .doc в виде .lo-layout; сам .doc не изменяется", "Save image positions and widths beside the .doc as a .lo-layout sidecar; does not edit Word .doc"),
                )
                .clicked()
            {
                self.save_legacy_layout();
                ui.close_menu();
            }
            ui.separator();
            if ui
                .add_enabled(
                    self.has_document() && self.pdf.is_none(),
                    egui::Button::new(tr!("Экспорт в TXT…", "Export to TXT…")),
                )
                .clicked()
            {
                self.export_text();
                ui.close_menu();
            }
            if ui
                .add_enabled(
                    self.docx.is_some() || self.legacy_doc.is_some() || self.spreadsheet.is_some(),
                    egui::Button::new(tr!(
                        "Создать копию без активного содержимого…",
                        "Create passive-only copy…"
                    )),
                )
                .on_hover_text(tr!(
                    "Только текст, таблицы, значения и растровые изображения; формулы, макросы, OLE/ActiveX, вложения и внешние связи не переносятся",
                    "Only text, tables, values and raster images; formulas, macros, OLE/ActiveX, attachments and external links are not transferred"
                ))
                .clicked()
            {
                self.create_clean_copy();
                ui.close_menu();
            }
            if ui
                .add_enabled(
                    self.spreadsheet.is_some(),
                    egui::Button::new(tr!("Экспорт CSV…", "Export CSV…")),
                )
                .clicked()
            {
                self.export_csv();
                ui.close_menu();
            }
            let can_export_pdf = cfg!(windows)
                && self.print_receiver.is_none()
                && (self.docx.is_some() || self.spreadsheet.is_some());
            if ui
                .add_enabled(
                    can_export_pdf,
                    egui::Button::new(tr!("Экспорт PDF…", "Export PDF…")),
                )
                .clicked()
            {
                self.export_pdf();
                ui.close_menu();
            }
            let can_export_html = self.docx.is_some() || self.spreadsheet.is_some();
            if ui
                .add_enabled(
                    can_export_html,
                    egui::Button::new(tr!("Экспорт HTML…", "Export HTML…")),
                )
                .clicked()
            {
                self.export_html();
                ui.close_menu();
            }
        });
    }

    fn view_menu(&mut self, ui: &mut egui::Ui) {
        flat_menu(ui, tr!("Вид", "View"), |ui| {
            ui.set_min_width(210.0);
            ui.menu_button(tr!("Тема", "Theme"), |ui| {
                ui.set_min_width(210.0);
                for theme in liteoffice::theme::Theme::ALL {
                    let label = if liteoffice::i18n::is_ru() {
                        theme.label_ru()
                    } else {
                        theme.label_en()
                    };
                    if ui.selectable_label(self.theme == theme, label).clicked() {
                        self.set_theme(theme);
                        ui.close_menu();
                    }
                }
            });
        });
    }

    fn edit_menu(&mut self, ui: &mut egui::Ui) {
        flat_menu(ui, tr!("Правки", "Edits"), |ui| {
            ui.set_min_width(230.0);
            if ui
                .add_enabled(
                    self.can_undo(),
                    egui::Button::new(tr!("Отменить", "Undo")).shortcut_text("Ctrl+Z"),
                )
                .clicked()
            {
                self.undo();
                ui.close_menu();
            }
            if ui
                .add_enabled(
                    self.can_redo(),
                    egui::Button::new(tr!("Повторить", "Redo")).shortcut_text("Ctrl+Shift+Z"),
                )
                .clicked()
            {
                self.redo();
                ui.close_menu();
            }
            ui.separator();
            if ui
                .add_enabled(
                    self.has_document(),
                    egui::Button::new(tr!("Найти…", "Find…")).shortcut_text("Ctrl+F"),
                )
                .clicked()
            {
                self.search_active = true;
                self.search_focus_requested = true;
                self.docx_focus_pending = false;
                ui.close_menu();
            }
            if ui
                .add_enabled(
                    self.has_document() && self.pdf.is_none() && self.text.is_none(),
                    egui::Button::new(tr!("Заменить…", "Replace…")).shortcut_text("Ctrl+H"),
                )
                .clicked()
            {
                self.search_active = true;
                self.replace_open = true;
                self.search_focus_requested = true;
                self.docx_focus_pending = false;
                ui.close_menu();
            }
            if ui
                .add_enabled(
                    self.docx.is_some() || self.text.is_some(),
                    egui::Button::new(tr!("Выделить всё", "Select all")).shortcut_text("Ctrl+A"),
                )
                .clicked()
            {
                if self.docx.is_some() {
                    self.select_all_docx = true;
                } else {
                    self.select_all_text = true;
                }
                ui.close_menu();
            }
            if self.legacy_doc.is_some() || self.spreadsheet.is_some() {
                ui.separator();
                if ui
                    .button(tr!("Редактировать как DOCX…", "Edit as DOCX…"))
                    .clicked()
                {
                    self.request_legacy_edit();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        self.spreadsheet
                            .as_ref()
                            .is_some_and(|book| book.editor.is_none()),
                        egui::Button::new(tr!(
                            "Править значения как копию XLSX…",
                            "Edit values as XLSX copy…"
                        )),
                    )
                    .clicked()
                {
                    self.edit_workbook_copy();
                    ui.close_menu();
                }
            }
        });
    }

    fn recent_menu_contents(&mut self, ui: &mut egui::Ui, recents: &[std::path::PathBuf]) {
        for path in recents {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            if ui
                .button(name)
                .on_hover_text(path.display().to_string())
                .clicked()
            {
                self.request_open_path(path.clone());
                ui.close_menu();
            }
        }
        if !recents.is_empty() {
            ui.separator();
        }
        if ui
            .add_enabled(
                !recents.is_empty(),
                egui::Button::new(tr!("Очистить список недавних файлов", "Clear recent files")),
            )
            .clicked()
        {
            self.clear_recent_files();
            ui.close_menu();
        }
        if ui
            .button(tr!("Открыть папку восстановления", "Open recovery folder"))
            .clicked()
        {
            self.open_recovery_folder();
            ui.close_menu();
        }
    }

    fn recent_menu_button(&mut self, ui: &mut egui::Ui) {
        let recents = self.recent_paths.clone();
        ui.menu_button(
            RichText::new(tr!("Недавние", "Recent")).size(11.0),
            |ui| {
                self.recent_menu_contents(ui, &recents);
            },
        );
    }

    // -- document strip ----------------------------------------------------

    pub(crate) fn shell_document_strip(&mut self, ctx: &egui::Context) {
        if !self.has_document() {
            return;
        }
        egui::TopBottomPanel::top("document-bar")
            .frame(theme::strip_frame())
            .show(ctx, |ui| {
                ui.set_min_height(theme::STRIP_HEIGHT - 10.0);
                if self.decision_pending() {
                    ui.disable();
                }
                // A plain horizontal row keeps this auto-sized panel stable.
                // `horizontal_centered` would expand to the previous panel
                // height and make the strip grow by a few points every frame.
                ui.horizontal(|ui| self.document_strip_row(ui));
                if self.search_visible() {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| self.show_search_bar(ui));
                }
                if self.replace_open {
                    ui.horizontal(|ui| self.show_replace_bar(ui));
                }
                if self
                    .spreadsheet
                    .as_ref()
                    .is_some_and(|book| book.editor.is_some())
                {
                    let before_edit = self.workbook_snapshot();
                    let changed = ui
                        .horizontal_wrapped(|ui| {
                            self.spreadsheet
                                .as_mut()
                                .unwrap()
                                .show_editor(ui, self.docx_editor_generation)
                        })
                        .inner;
                    if changed {
                        self.record_workbook_edit(before_edit);
                        self.search_dirty = true;
                    }
                }
                ui.add_space(3.0);
                theme::bottom_rule(ui);
            });
    }

    fn document_strip_row(&mut self, ui: &mut egui::Ui) {
        let (badge, badge_fg, badge_bg) = self.document_badge();
        let badge_hint = if let Some(book) = &self.spreadsheet {
            if book.editor.is_some() {
                Some(tr!("Оригинал защищён. Восстановление книги после сбоя включено; сохранение — только отдельной копией XLSX.", "Original protected. Workbook crash recovery is enabled; save a separate XLSX copy."))
            } else {
                Some(tr!("Копия значений: числа, даты и логические значения пишутся типами, остальное — текстом. Формулы, сложные форматы, таблицы, диаграммы и разметка не сохраняются.", "Values copy: numbers, dates and booleans are typed, the rest stays text. Formulas, complex formats, tables, charts and layout are not retained."))
            }
        } else if self.legacy_doc.is_some() {
            Some(tr!("Преобразование DOC: только восстановленный текст; исходные стили и таблицы не сохраняются. Разрывы строк становятся абзацами; изображения без позиций — в конце.", "DOC conversion: recovered text only; original styles and tables are not retained. Line breaks become paragraphs; unplaced pictures go at the end."))
        } else if self
            .docx
            .as_ref()
            .is_some_and(|package| package.requires_save_as())
        {
            Some(tr!("Преобразованная копия DOC: текст и кегль редактируются; требуется «Сохранить как .docx». Оригинал DOC защищён.", "Converted DOC copy: text and font size are editable; Save As .docx required. Original DOC protected."))
        } else {
            None
        };
        let badge_response = theme::chip(ui, badge, badge_fg, badge_bg);
        if let Some(hint) = badge_hint {
            badge_response.on_hover_text(hint);
        }

        let (name, full_path) = self.document_title();
        let name_width = (ui.available_width() * 0.30).clamp(80.0, 300.0);
        let title = ui
            .allocate_ui_with_layout(
                Vec2::new(name_width, 18.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.set_max_width(name_width);
                    ui.add(
                        egui::Label::new(RichText::new(name).size(12.5).color(theme::ink()))
                            .truncate(),
                    )
                },
            )
            .inner;
        title.on_hover_text(full_path);

        if self.is_dirty() {
            theme::dot(ui, theme::warn());
            ui.label(
                RichText::new(tr!("Не сохранено", "Unsaved"))
                    .size(10.5)
                    .color(theme::warn()),
            );
        }
        if self.recovered_document {
            theme::chip(
                ui,
                tr!("ВОССТАНОВЛЕНО", "RECOVERED"),
                theme::warn(),
                theme::warn_soft(),
            )
            .on_hover_text(tr!(
                "Восстановленная копия — перед заменой источника требуется «Сохранить как».",
                "Recovered copy — Save As is required before the source can be replaced."
            ));
        }
        if self.text.is_some() {
            let recovery_available = self.recovery_root.is_some();
            let (color, hint) = if recovery_available {
                (
                    theme::accent(),
                    tr!(
                        "Восстановление включено: каждые 2 секунды сохраняется контрольная точка. После сбоя можно восстановить копию; оригинал не изменяется.",
                        "Recovery is on: a checkpoint is saved every 2 seconds. After a crash you can restore a copy; the original is unchanged."
                    ),
                )
            } else {
                (
                    theme::warn(),
                    tr!(
                        "Восстановление недоступно в этом окне: контрольные точки не создаются. Сохраняйте работу вручную; оригинал не изменяется.",
                        "Recovery is unavailable in this window: checkpoints are not created. Save your work manually; the original is unchanged."
                    ),
                )
            };
            theme::dot(ui, color).on_hover_text(hint);
        }

        ui.add_space(2.0);
        theme::hairline(ui);
        ui.add_space(2.0);

        if self.docx.is_some() {
            self.strip_font_controls(ui);
        }
        if let Some(document) = &self.text {
            let has_text = !document.text.is_empty();
            if ui
                .add_enabled(
                    has_text,
                    egui::Button::new(RichText::new(tr!("Выделить всё", "Select all")).size(11.5)),
                )
                .on_hover_text(tr!(
                    "Выделить весь текст (Ctrl+A)",
                    "Select all text (Ctrl+A)"
                ))
                .clicked()
            {
                self.select_all_text = true;
            }
            ui.label(
                RichText::new(tr!("Шрифт", "Font"))
                    .size(10.5)
                    .color(theme::faint()),
            );
            ui.label(RichText::new("—").size(11.0).color(theme::faint()))
                .on_hover_text(tr!(
                    "TXT, MD и RTF не хранят шрифт. Кегль доступен для DOCX; для текста используйте масштаб.",
                    "TXT, MD and RTF do not store a font. Font size is available for DOCX; use zoom for text."
                ));
        }
        if self.spreadsheet.is_some() {
            self.strip_sheet_controls(ui);
        }
        if self.legacy_doc.is_some() {
            ui.label(
                RichText::new(tr!("DOC — только просмотр", "DOC is view-only"))
                    .size(10.5)
                    .color(theme::muted()),
            );
            if ui
                .add(egui::Button::new(
                    RichText::new(tr!("Редактировать как DOCX…", "Edit as DOCX…")).size(11.5),
                ))
                .on_hover_text(tr!(
                    "Создать редактируемую DOCX-копию из восстановленного текста; оригинальный .doc не изменится",
                    "Create an editable DOCX copy from recovered text; the original .doc stays unchanged"
                ))
                .clicked()
            {
                self.request_legacy_edit();
            }
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let search_shown = self.search_visible();
            if ui
                .add(
                    theme::IconButton::new(Icon::Search, tr!("Найти (Ctrl+F)", "Find (Ctrl+F)"))
                        .selected(search_shown)
                        .small(),
                )
                .clicked()
            {
                if search_shown {
                    self.search_active = false;
                } else {
                    self.search_active = true;
                    self.search_focus_requested = true;
                    self.docx_focus_pending = false;
                }
            }
            if let Some(package) = &self.docx {
                let pages = docx_preview_plan(
                    &package.model,
                    &package.images,
                    self.print_settings.sanitized(),
                    self.zoom,
                )
                .1
                .len();
                if pages > 0 {
                    ui.label(
                        RichText::new(tfm!("Страниц: {pages}", "{pages} pages"))
                            .size(10.5)
                            .color(theme::faint()),
                    );
                }
            }
            if self.pdf.is_some() {
                ui.label(
                    RichText::new(tr!("растровый просмотр", "raster view"))
                        .size(10.5)
                        .color(theme::faint()),
                );
            }
        });
    }

    fn strip_font_controls(&mut self, ui: &mut egui::Ui) {
        let (shown_size, has_selection) = match self.docx.as_ref() {
            Some(package) => {
                let shown = if self.select_all_docx {
                    package.model.common_font_size()
                } else {
                    self.selected_fragment_id
                        .and_then(|id| package.model.fragment(id))
                        .and_then(|fragment| fragment.font_size_half_points)
                };
                (
                    shown,
                    self.select_all_docx || self.selected_fragment_id.is_some(),
                )
            }
            None => (None, false),
        };
        if !has_selection {
            self.font_input_id = None;
            ui.label(
                RichText::new(tr!("Шрифт", "Font"))
                    .size(10.5)
                    .color(theme::faint()),
            );
            ui.label(RichText::new("—").size(11.0).color(theme::faint()))
                .on_hover_text(tr!(
                    "Выделите текст в документе, чтобы изменить кегль",
                    "Select text in the document to change its size"
                ));
        } else {
            let mut target = shown_size.unwrap_or(24);
            let mut apply = None;
            ui.label(
                RichText::new(tr!("Шрифт", "Font"))
                    .size(10.5)
                    .color(theme::faint()),
            );
            if ui
                .add(
                    theme::IconButton::new(
                        Icon::Minus,
                        tr!("Уменьшить кегль", "Decrease font size"),
                    )
                    .small(),
                )
                .clicked()
            {
                apply = Some(target.saturating_sub(2).max(12));
                target = target.saturating_sub(2).max(12);
            }
            let mut points = target as f32 / 2.0;
            let input = ui.add(
                egui::DragValue::new(&mut points)
                    .range(6.0..=72.0)
                    .speed(0.5)
                    .suffix(" pt"),
            );
            self.font_input_id = Some(input.id);
            if input.clicked() {
                self.docx_focus_pending = false;
            }
            if input.changed() {
                let half_points = (points * 2.0).round().clamp(12.0, 144.0) as u16;
                apply = Some(half_points);
            }
            if ui
                .add(
                    theme::IconButton::new(
                        Icon::Plus,
                        tr!("Увеличить кегль", "Increase font size"),
                    )
                    .small(),
                )
                .clicked()
            {
                apply = Some(target.saturating_add(2).min(144));
            }
            if let Some(size) = apply {
                self.change_font_size(size);
            }
        }
        if !self.select_all_docx
            && ui
                .add(
                    egui::Button::new(
                        RichText::new(tr!("Выделить всё", "Select all"))
                            .size(10.5)
                            .color(theme::muted()),
                    )
                    .frame(false),
                )
                .on_hover_text(tr!(
                    "Выделить все абзацы (Ctrl+A при фокусе на документе)",
                    "Select every paragraph (Ctrl+A while the document has focus)"
                ))
                .clicked()
        {
            self.select_all_docx = true;
        }
        if self.select_all_docx {
            ui.label(
                RichText::new(tr!("весь текст", "all text"))
                    .size(10.5)
                    .color(theme::accent()),
            );
        }
    }

    fn strip_sheet_controls(&mut self, ui: &mut egui::Ui) {
        let book = self.spreadsheet.as_mut().unwrap();
        let selected_name = book
            .selected_sheet()
            .map(|sheet| sheet.sheet_name.as_str())
            .unwrap_or(tr!("Лист", "Sheet"));
        let names = book.sheet_names().map(str::to_owned).collect::<Vec<_>>();
        let previous = book.selected_sheet;
        egui::ComboBox::from_id_source("document_sheet")
            .selected_text(RichText::new(selected_name).size(11.5))
            .width(130.0)
            .show_ui(ui, |ui| {
                for (index, name) in names.iter().enumerate() {
                    ui.selectable_value(&mut book.selected_sheet, index, name);
                }
            });
        if book.selected_sheet != previous {
            book.selected_cell = None;
            book.preview_anchor = None;
            self.search_scroll_requested = false;
        }
        let editing = book.editor.is_some();
        if !editing
            && ui
                .add(egui::Button::new(
                    RichText::new(tr!("Изменить значения", "Edit values"))
                        .size(11.5)
                        .color(theme::accent_ink()),
                ))
                .on_hover_text(tr!(
                    "Создать копию XLSX с типизированными значениями ячеек; формулы не сохраняются",
                    "Create an XLSX copy with typed cell values; formulas are not kept"
                ))
                .clicked()
        {
            self.edit_workbook_copy();
        }
    }

    // -- status line -------------------------------------------------------

    pub(crate) fn shell_status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(theme::STATUS_HEIGHT)
            .frame(theme::status_frame())
            .show(ctx, |ui| {
                theme::top_rule(ui);
                ui.horizontal_centered(|ui| {
                    let message = if self.status.is_empty() {
                        if self.has_document() {
                            tr!("Готово", "Ready")
                        } else {
                            tr!("Документ не открыт", "No document open")
                        }
                    } else {
                        self.status.as_str()
                    };
                    let color = if self.status.starts_with("Не удалось открыть")
                        || self.status.starts_with("Не удалось сохранить")
                        || self.status.starts_with("Open failed")
                        || self.status.starts_with("Save failed")
                    {
                        theme::danger()
                    } else if self.status.is_empty() {
                        theme::faint()
                    } else {
                        theme::muted()
                    };
                    let message_width = (ui.available_width() * 0.55).max(120.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(message_width, 16.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.set_max_width(message_width);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(message).size(11.0).color(color),
                                )
                                .truncate(),
                            )
                            .on_hover_text(message);
                        },
                    );

                    if !self.search_results.hits.is_empty() {
                        ui.label(
                            RichText::new(self.search_context())
                                .size(11.0)
                                .color(theme::accent_ink()),
                        );
                    }
                    if !self.recovery_notice.is_empty() {
                        let notice_width = (ui.available_width() * 0.25).max(80.0);
                        ui.allocate_ui_with_layout(
                            Vec2::new(notice_width, 16.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.set_max_width(notice_width);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&self.recovery_notice)
                                            .size(11.0)
                                            .color(theme::warn()),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&self.recovery_notice);
                            },
                        );
                    }
                    if self.print_receiver.is_some() {
                        ui.label(RichText::new(tr!("Идёт печать…", "Printing…")).size(11.0).color(theme::muted()));
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if !self.open_diagnostics.is_empty() {
                            ui.menu_button(
                                RichText::new(tr!("Подробнее", "Details")).size(11.0).color(theme::muted()),
                                |ui| {
                                    ui.set_min_width(360.0);
                                    for message in &self.open_diagnostics {
                                        ui.label(RichText::new(message).size(11.5));
                                    }
                                },
                            );
                        }
                        if self.print_receiver.is_some() {
                            ui.label(RichText::new(tr!("Диалог печати открыт", "Print dialog active")).size(10.5).color(theme::faint()));
                        }
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                        theme::paint_icon(ui.painter(), rect, Icon::Lock, theme::faint());
                        let hint = response.on_hover_text(
                            tr!("Только локальные файлы. Скрипты документов, макросы и внешнее содержимое никогда не запускаются.", "Local files only. Document scripts, macros and external content are never enabled."),
                        );
                        let _ = hint;
                        ui.label(RichText::new(tr!("локально", "local")).size(10.5).color(theme::faint()));
                    });
                });
            });
    }

    // -- page setup --------------------------------------------------------

    pub(crate) fn shell_page_setup(&mut self, ctx: &egui::Context) {
        if !self.page_setup_open {
            return;
        }
        let mut open = true;
        let title = theme::heading_font(ctx, 13.0);
        egui::Window::new(RichText::new(tr!("Параметры печати", "Print setup")).font(title))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(330.0)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        tr!("Влияет на разбиение на страницы и печать. Исходный файл не изменяется.", "Affects preview pagination and printing. The original file is never changed."),
                    )
                    .size(11.0)
                    .color(theme::muted()),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr!("Ориентация", "Orientation")).size(11.5).color(theme::muted()));
                    ui.selectable_value(&mut self.print_settings.landscape, false, tr!("Книжная", "Portrait"));
                    ui.selectable_value(&mut self.print_settings.landscape, true, tr!("Альбомная", "Landscape"));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(tr!("Поля печати", "Print margins"))
                            .size(11.5)
                            .color(theme::muted()),
                    )
                    .on_hover_text(tr!(
                        "Влияют на печать и разбиение. Вёрстка исходного DOCX не меняется; поля для произвольного фрагмента текста форматом DOCX не поддерживаются.",
                        "Affect printing and pagination. The original DOCX layout is unchanged; per-fragment margins are not supported by the DOCX format."
                    ));
                    ui.add(
                        egui::DragValue::new(&mut self.print_settings.margin_mm)
                            .range(5.0..=40.0)
                            .speed(0.5)
                            .suffix(" mm"),
                    )
                    .on_hover_text(tr!(
                        "Общие поля страницы для печати, не для выделенного текста",
                        "Page margins for printing, not for selected text"
                    ));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr!("Изображения при печати", "Images when printing")).size(11.5).color(theme::muted()));
                    egui::ComboBox::from_id_source("page_setup_scale")
                        .selected_text(match self.print_settings.image_scale {
                            liteoffice::print::ImageScale::OriginalSize => tr!("Оригинальный размер", "Original size"),
                            liteoffice::print::ImageScale::FitToWidth => tr!("По ширине", "Fit to width"),
                            liteoffice::print::ImageScale::CustomPercent => tr!("Ширина из DOCX", "Width from DOCX"),
                        })
                        .width(130.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.print_settings.image_scale,
                                liteoffice::print::ImageScale::OriginalSize,
                                tr!("Оригинальный размер", "Original size"),
                            );
                            ui.selectable_value(
                                &mut self.print_settings.image_scale,
                                liteoffice::print::ImageScale::FitToWidth,
                                tr!("По ширине", "Fit to width"),
                            );
                            ui.selectable_value(
                                &mut self.print_settings.image_scale,
                                liteoffice::print::ImageScale::CustomPercent,
                                tr!("Ширина из DOCX", "Width from DOCX"),
                            );
                        })
                        .response
                        .on_hover_text(tr!(
                            "Ширина берётся из самого DOCX для каждого изображения. Общего процента в этой версии нет.",
                            "Width is taken from each image in the DOCX. There is no global percentage in this version."
                        ));
                });
                ui.add_space(4.0);
                ui.checkbox(
                    &mut self.print_settings.print_page_numbers,
                    tr!("Номера страниц", "Page numbers"),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button(tr!("Готово", "Done")).clicked() {
                        self.page_setup_open = false;
                    }
                    ui.label(
                        RichText::new(tr!(
                            "Печать — кнопкой на панели или Ctrl+P",
                            "Print from the toolbar or Ctrl+P"
                        ))
                        .size(10.5)
                        .color(theme::faint()),
                    );
                    if self.pdf.is_some() {
                        ui.label(
                            RichText::new(tr!("PDF использует собственный диалог печати", "PDF uses its own print dialog"))
                                .size(10.5)
                                .color(theme::faint()),
                        );
                    }
                });
            });
        if !open {
            self.page_setup_open = false;
        }
    }

    // -- start screen ------------------------------------------------------

    pub(crate) fn shell_empty_state(&mut self, ui: &mut egui::Ui) {
        let top_space = (ui.available_height() * 0.11).clamp(20.0, 90.0);
        ui.vertical_centered(|ui| {
            ui.add_space(top_space);
            theme::brand_mark(ui, 50.0, true);
            ui.add_space(12.0);
            ui.label(
                RichText::new("NotOffice")
                    .size(23.0)
                    .family(theme::heading_family(ui.ctx()))
                    .color(theme::ink()),
            );
            ui.add_space(3.0);
            ui.label(
                RichText::new(tr!(
                    "Читайте и редактируйте документы, не отправляя их никуда.",
                    "Read and edit office documents without sending them anywhere."
                ))
                .size(12.0)
                .color(theme::muted()),
            );
            ui.add_space(20.0);
            // A fixed-width row is centred by the enclosing layout; a bare
            // `ui.horizontal` would stretch and push the buttons to the left.
            ui.allocate_ui_with_layout(
                Vec2::new(330.0, 30.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    if ui
                        .add(theme::PrimaryButton::new(tr!(
                            "Открыть документ…",
                            "Open a document…"
                        )))
                        .clicked()
                    {
                        self.open_document();
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(tr!("Новый текст", "New text"))
                                    .size(12.5)
                                    .color(theme::ink()),
                            )
                            .fill(theme::card())
                            .stroke(Stroke::new(1.0_f32, theme::line()))
                            .rounding(egui::Rounding::same(6.0))
                            .min_size(Vec2::new(0.0, 27.0)),
                        )
                        .on_hover_text(tr!(
                            "Начать пустой текстовый документ в этом окне",
                            "Start an empty text document in this window"
                        ))
                        .clicked()
                    {
                        self.start_blank_text();
                    }
                },
            );
            ui.add_space(9.0);
            ui.label(
                RichText::new(tr!(
                    "Перетащите файл сюда  ·  Shift+перетаскивание откроет новое окно",
                    "Drop a file here  ·  Shift+drop opens a new window"
                ))
                .size(10.5)
                .color(theme::faint()),
            );
            ui.add_space(10.0);
            ui.label(
                RichText::new(
                    "DOCX · DOC · XLSX · XLS · XLSB · ODS · CSV · TSV · PDF · TXT · MD · RTF",
                )
                .size(9.5)
                .color(theme::faint()),
            );

            let recents = self
                .recent_paths
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>();
            if recents.is_empty() {
                ui.add_space(22.0);
                ui.label(
                    RichText::new(tr!(
                        "Здесь появятся недавно открытые файлы.",
                        "Recently opened files will appear here."
                    ))
                    .size(10.5)
                    .color(theme::faint()),
                );
            }
            if !recents.is_empty() {
                ui.add_space(26.0);
                egui::Frame::none()
                    .fill(theme::card())
                    .stroke(Stroke::new(1.0_f32, theme::line()))
                    .rounding(egui::Rounding::same(9.0))
                    .shadow(egui::epaint::Shadow {
                        offset: egui::vec2(0.0, 4.0),
                        blur: 16.0,
                        spread: 0.0,
                        color: egui::Color32::from_black_alpha(20),
                    })
                    .inner_margin(egui::Margin::symmetric(14.0, 12.0))
                    .show(ui, |ui| {
                        ui.set_width(420.0);
                        ui.with_layout(Layout::top_down(Align::Min), |ui| {
                            ui.label(
                                RichText::new(tr!("НЕДАВНИЕ", "RECENT"))
                                    .size(9.5)
                                    .family(theme::heading_family(ui.ctx()))
                                    .color(theme::faint()),
                            );
                            ui.add_space(6.0);
                            for path in recents.into_iter() {
                                let name = path
                                    .file_name()
                                    .map(|name| name.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| path.display().to_string());
                                ui.horizontal(|ui| {
                                    let (icon_rect, _) =
                                        ui.allocate_exact_size(Vec2::splat(13.0), Sense::hover());
                                    theme::paint_icon(
                                        ui.painter(),
                                        icon_rect,
                                        Icon::Page,
                                        theme::faint(),
                                    );
                                    let response = ui
                                        .allocate_ui_with_layout(
                                            Vec2::new(360.0, 22.0),
                                            Layout::left_to_right(Align::Center),
                                            |ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(name)
                                                            .size(12.0)
                                                            .color(theme::ink()),
                                                    )
                                                    .truncate()
                                                    .sense(Sense::click()),
                                                )
                                            },
                                        )
                                        .inner;
                                    if response.hovered() {
                                        ui.painter().line_segment(
                                            [
                                                egui::pos2(
                                                    response.rect.left(),
                                                    response.rect.bottom() + 1.0,
                                                ),
                                                egui::pos2(
                                                    response.rect.right(),
                                                    response.rect.bottom() + 1.0,
                                                ),
                                            ],
                                            Stroke::new(1.0_f32, theme::accent()),
                                        );
                                    }
                                    if response
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .on_hover_text(path.display().to_string())
                                        .clicked()
                                    {
                                        self.request_open_path(path);
                                    }
                                });
                            }
                        });
                    });
            }
            ui.add_space(20.0);
            ui.label(
                RichText::new(tfm!("Версия {}", "Version {}", env!("CARGO_PKG_VERSION")))
                    .size(9.5)
                    .color(theme::faint()),
            );
        });
    }
}
