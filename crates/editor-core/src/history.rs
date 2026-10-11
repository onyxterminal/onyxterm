use crate::buffer::Buffer;
use crate::edit::Edit;
use crate::selection::Selections;
use std::time::{Duration, Instant};

const TYPING_GROUP_WINDOW: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditKind {
    Typing,
    Other,
}

#[derive(Debug, Clone)]
struct Transaction {
    steps: Vec<(Edit, Edit)>,
    before: Selections,
    after: Selections,
    kind: EditKind,
    at: Instant,
}

#[derive(Debug, Default)]
pub(crate) struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
}

pub(crate) struct Record {
    pub forward: Edit,
    pub inverse: Edit,
    pub before: Selections,
    pub after: Selections,
    pub kind: EditKind,
    pub at: Instant,
}

impl History {
    pub(crate) fn record(&mut self, record: Record) {
        self.redo.clear();
        if let Some(last) = self.undo.last_mut() {
            let is_continuation = record.kind == EditKind::Typing
                && last.kind == EditKind::Typing
                && record.at.duration_since(last.at) <= TYPING_GROUP_WINDOW
                && last.after == record.before;
            if is_continuation {
                last.steps.push((record.forward, record.inverse));
                last.after = record.after;
                last.at = record.at;
                return;
            }
        }
        self.undo.push(Transaction {
            steps: vec![(record.forward, record.inverse)],
            before: record.before,
            after: record.after,
            kind: record.kind,
            at: record.at,
        });
    }

    pub(crate) fn undo(&mut self, buffer: &mut Buffer) -> Option<Selections> {
        let transaction = self.undo.pop()?;
        for (_, inverse) in transaction.steps.iter().rev() {
            inverse.apply(buffer);
        }
        let selections = transaction.before.clone();
        self.redo.push(transaction);
        Some(selections)
    }

    pub(crate) fn redo(&mut self, buffer: &mut Buffer) -> Option<Selections> {
        let transaction = self.redo.pop()?;
        for (forward, _) in &transaction.steps {
            forward.apply(buffer);
        }
        let selections = transaction.after.clone();
        self.undo.push(transaction);
        Some(selections)
    }
}
