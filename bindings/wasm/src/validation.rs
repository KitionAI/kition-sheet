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
    step: Option<(i64, i64)>,
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

    /// Enters what is typed and moves by (`dr`, `dc`), as Enter, Tab and the arrows do; held by
    /// the cell's validation, it stays and the move waits for the answer.
    pub(crate) fn enter_and_step(&mut self, dr: i64, dc: i64) -> Result<(), String> {
        self.commit()?;
        if self.editing.is_some() {
            if let Some(held) = self.validation_held.as_mut() {
                held.step = Some((dr, dc));
            }
            return Ok(());
        }
        if (dr, dc) == (0, 0) {
            return Ok(());
        }
        self.step(dr, dc, false, false)
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
                    && let Some((dr, dc)) = held.and_then(|h| h.step)
                {
                    self.step(dr, dc, false, false)?;
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
