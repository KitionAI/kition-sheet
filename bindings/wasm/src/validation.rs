//! Data validation in the editor, as WPS shows it: what is typed into a cell its rule refuses is
//! held in the cell and the editor asks (`dialog` `validationError`): a Stop rule's message with
//! 重试 and 取消, a Warning's with 是, 否 and 取消, an Information's with 确定 and 取消; the answer
//! comes back as `editor.validation`. The active cell's rule shows its input message and, for a
//! list, the arrow that opens its entries (`inspect().validation`).

use kition_sheet_engine::model::{ErrorStyle, Validation, ValidationKind};

use super::*;

/// What is typed, held by its cell's data validation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Held {
    /// The rule's error style: a Stop never lets it in.
    stop: bool,
    /// Where Enter (Tab, an arrow) would have moved once it goes in.
    step: Option<Step>,
}

/// Where the active cell goes after Enter, Tab or an arrow (with or without typing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// An arrow: by rows and columns.
    By(i64, i64),
    /// Enter (`true`: Shift+Enter, up).
    Enter(bool),
    /// Tab (`true`: Shift+Tab, left).
    Tab(bool),
}

/// A run of Tabs along a row (WPS W-B3): the column it started in, and where its last Tab
/// landed. Enter there goes to the row below at the starting column; any other move ends it.
/// Session state: never saved.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TabRun {
    sheet: usize,
    start: u32,
    at: CellRef,
}

impl SheetDocument {
    /// What is typed was refused by `rule` (`message`): it stays in the cell and the editor asks.
    /// `quiet`: the caller drops it instead (leaving the sheet).
    pub(crate) fn hold_for_validation(&mut self, rule: &Validation, message: String, cell: CellRef) {
        let style = match rule.error_style {
            ErrorStyle::Stop => "stop",
            ErrorStyle::Warning => "warning",
            ErrorStyle::Information => "information",
        };
        self.validation_held = Some(Held { stop: rule.error_style == ErrorStyle::Stop, step: None });
        let params = json!({ "style": style, "title": rule.error_title, "message": message, "cell": cell.a1() });
        self.dialog_asked = (self.dialog_asked.0 + 1, Some(("validationError".into(), params, "editor.validation".into())));
        self.notice = (self.notice.0 + 1, Some(message));
        self.edits += 1;
    }

    /// Enters what is typed and moves as Enter, Tab and the arrows do (`step_after`); held by the
    /// cell's validation, it stays and the move waits for the answer.
    pub(crate) fn enter_and_step(&mut self, step: Step) -> Result<(), String> {
        self.commit()?;
        if self.editing.is_some() {
            if let Some(held) = self.validation_held.as_mut() {
                held.step = Some(step);
            }
            return Ok(());
        }
        self.step_after(step)
    }

    /// Moves the active cell as Enter, Tab or an arrow does, as WPS does (W-B3, W-B4): in a
    /// selection of more than one cell, Enter and Tab go round its cells and the selection stays
    /// (`select.next`); Tab remembers the column a run of Tabs started in, and Enter at the end of
    /// the run goes to the row below at that column; any other move ends the run.
    pub(crate) fn step_after(&mut self, step: Step) -> Result<(), String> {
        let run = self.tab_run.take();
        let Ok(doc) = self.session.doc() else { return Ok(()) };
        let (sheet, active, single) = (doc.wb.active_sheet, doc.selection.active, doc.selection.is_single_cell());
        let run = run.filter(|run| run.sheet == sheet && run.at == active);
        match step {
            Step::By(0, 0) => Ok(()),
            Step::By(dr, dc) => self.step(dr, dc, false, false),
            Step::Enter(back) | Step::Tab(back) if !single => self
                .session
                .execute("select.next", json!({ "forward": !back, "byRow": matches!(step, Step::Tab(_)) }))
                .map(|_| ())
                .map_err(engine_error),
            Step::Tab(back) => {
                let start = run.map_or(active.col, |run| run.start);
                self.step(0, if back { -1 } else { 1 }, false, false)?;
                self.tab_run = Some(TabRun { sheet, start, at: self.active() });
                Ok(())
            }
            Step::Enter(false) if run.is_some() => self.return_below(run.map_or(active.col, |run| run.start)),
            Step::Enter(back) => self.step(if back { -1 } else { 1 }, 0, false, false),
        }
    }

    /// Enter at the end of a run of Tabs: the row below the active cell (below its merge, past
    /// hidden rows), at column `col`.
    fn return_below(&mut self, col: u32) -> Result<(), String> {
        let area = {
            let Some((_, sheet)) = self.sheet() else { return Ok(()) };
            let active = self.active();
            let mut row = sheet.merge_at(active).map_or(active.row, |m| m.end.row).saturating_add(1).min(MAX_ROWS - 1);
            let mut guard = 0;
            while sheet.is_row_hidden(row) && row + 1 < MAX_ROWS && guard < 100_000 {
                row += 1;
                guard += 1;
            }
            let target = CellRef::new(row, col.min(MAX_COLS - 1));
            sheet.merge_at(target).unwrap_or(RangeRef::cell(target))
        };
        self.select(area, area.start)
    }

    /// Cmd+Enter while a cell is typed (WPS W-C10; walkthrough D14): what is typed goes into every
    /// cell of the selection (`range.fill`, a formula's relative references moving per cell), and
    /// the active cell and the selection stay. The active cell's validation holds it as Enter's
    /// does. A formula typed for another sheet's cell goes into its own cell only.
    pub(crate) fn fill_selection(&mut self) -> Result<(), String> {
        if self.editing_elsewhere() {
            return self.enter_and_step(Step::By(0, 0));
        }
        self.point = None;
        self.hint_pick = 0;
        let Some(text) = self.editing.as_ref().map(Edit::text) else { return Ok(()) };
        let (sheet, cell) = self.edit_home();
        if !text.starts_with('=')
            && !self.validation_skip
            && let Some((wb, _)) = self.sheet()
            && let Some((rule, message)) = kition_sheet_engine::cmd::data::check_validation(wb, sheet, cell, &text)
        {
            self.hold_for_validation(&rule, message, cell);
            return Ok(());
        }
        self.validation_held = None;
        self.editing = None;
        self.edits += 1;
        self.go_home()?;
        self.session.execute("range.fill", json!({ "input": text })).map(|_| ()).map_err(engine_error)
    }

    /// Enters what is typed, or drops it when its cell's validation holds it (leaving the sheet,
    /// inserting rows: nothing is asked).
    pub(crate) fn commit_or_drop(&mut self) {
        let asked = self.dialog_asked.clone();
        let notice = self.notice.clone();
        if self.commit().is_err() || self.editing.is_some() {
            self.editing = None;
            self.edit_home = None;
            self.validation_held = None;
            self.dialog_asked = asked;
            self.notice = notice;
            self.edits += 1;
        }
    }

    /// `editor.validation {answer}`: the person's answer to a validation message. `retry` (重试,
    /// 否) keeps the cell open with its text selected; `cancel` (取消) drops what was typed;
    /// `accept` (是, 确定; a Warning or an Information rule only) enters it and moves on.
    pub(crate) fn validation_answer(&mut self, params: &Json) -> Result<Json, String> {
        let answer = params.get("answer").and_then(Json::as_str).unwrap_or("retry");
        let held = self.validation_held.take();
        match answer {
            "cancel" => {
                if self.editing.take().is_some() {
                    self.edits += 1;
                }
                self.point = None;
                self.hint_pick = 0;
                self.go_home()?;
            }
            "accept" if held.is_some_and(|h| !h.stop) => {
                self.validation_skip = true;
                let entered = self.commit();
                self.validation_skip = false;
                entered?;
                if self.editing.is_none()
                    && let Some(step) = held.and_then(|h| h.step)
                {
                    self.step_after(step)?;
                }
            }
            "accept" | "retry" => {
                self.validation_held = held;
                if let Some(edit) = self.editing.as_mut() {
                    let end = edit.len();
                    edit.move_to(0, false);
                    edit.move_to(end, true);
                    edit.in_place = true;
                }
                self.edits += 1;
            }
            other => return Err(format!("`answer` is retry, accept or cancel, not `{other}`")),
        }
        Ok(json!({ "editing": self.editing.is_some() }))
    }

    /// The active cell's data validation as the editor shows it: its rectangle (document pixels),
    /// its list's entries when it offers them in a dropdown, its input message, and the rule as
    /// the Data Validation dialog edits it.
    pub(crate) fn validation_info(&self) -> Json {
        let Some((wb, sheet)) = self.sheet() else { return Json::Null };
        let Some(st) = self.session.active() else { return Json::Null };
        let active = st.selection.active;
        let Some(rule) = sheet.validations.iter().find(|d| d.ranges.iter().any(|r| r.contains(active))) else { return Json::Null };
        let area = sheet.merge_at(active).unwrap_or(RangeRef::cell(active));
        let (x0, y0, x1, y1) = Self::cell_rect(sheet, area);
        let list = (rule.kind == ValidationKind::List && rule.in_cell_dropdown)
            .then(|| kition_sheet_engine::cmd::data::list_items(wb, wb.active_sheet, rule).into_iter().take(10_000).collect::<Vec<_>>());
        let prompt = (rule.show_input && (!rule.input_title.is_empty() || !rule.input_message.is_empty()))
            .then(|| json!({ "title": rule.input_title, "message": rule.input_message }));
        json!({
            "cell": active.a1(),
            "rect": [x0, y0, x1, y1],
            "list": list,
            "prompt": prompt,
            "rule": rule_json(rule),
        })
    }
}

/// A rule in `data.validation`'s own terms.
fn rule_json(rule: &Validation) -> Json {
    let kind = match rule.kind {
        ValidationKind::Any => "any",
        ValidationKind::Whole => "whole",
        ValidationKind::Decimal => "decimal",
        ValidationKind::List => "list",
        ValidationKind::Date => "date",
        ValidationKind::Time => "time",
        ValidationKind::TextLength => "textLength",
        ValidationKind::Custom => "custom",
    };
    let operator = format!("{:?}", rule.op);
    let operator = {
        let mut chars = operator.chars();
        chars.next().map(|c| c.to_ascii_lowercase().to_string() + chars.as_str()).unwrap_or_default()
    };
    json!({
        "range": rule.ranges.iter().map(RangeRef::a1).collect::<Vec<_>>().join(","),
        "type": kind,
        "operator": operator,
        "formula1": rule.f1,
        "formula2": rule.f2,
        "ignoreBlank": rule.allow_blank,
        "dropdown": rule.in_cell_dropdown,
        "showInput": rule.show_input,
        "inputTitle": rule.input_title,
        "inputMessage": rule.input_message,
        "showError": rule.show_error,
        "errorStyle": match rule.error_style {
            ErrorStyle::Stop => "stop",
            ErrorStyle::Warning => "warning",
            ErrorStyle::Information => "information",
        },
        "errorTitle": rule.error_title,
        "errorMessage": rule.error_message,
    })
}
