use crate::shell_marks::ShellMark;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const BLOCK_LIMIT: usize = 500;
const INTERRUPTED_EXIT_CODE: i32 = 130;
const SUCCESS_EXIT_CODE: i32 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockOutcome {
    pub exit_code: Option<i32>,
    pub duration: Duration,
}

impl BlockOutcome {
    pub fn is_interrupted(&self) -> bool {
        self.exit_code == Some(INTERRUPTED_EXIT_CODE)
    }

    pub fn is_failure(&self) -> bool {
        self.exit_code.is_some_and(|code| code != SUCCESS_EXIT_CODE) && !self.is_interrupted()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockStatus {
    Running,
    Finished(BlockOutcome),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandBlock {
    pub id: BlockId,
    pub command: String,
    pub status: BlockStatus,
    started: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShellPhase {
    #[default]
    Unknown,
    Prompt,
    Editing(BlockId),
    Running(BlockId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockChange {
    InputStarted(BlockId),
    CommandStarted(BlockId),
    CommandFinished(CommandBlock),
}

#[derive(Debug, Default)]
pub struct CommandBlocks {
    next_id: u32,
    phase: ShellPhase,
    blocks: VecDeque<CommandBlock>,
}

impl CommandBlocks {
    pub fn phase(&self) -> ShellPhase {
        self.phase
    }

    pub fn block(&self, id: BlockId) -> Option<&CommandBlock> {
        self.blocks.iter().rev().find(|block| block.id == id)
    }

    pub fn last_outcome(&self) -> Option<BlockOutcome> {
        match self.blocks.back()?.status {
            BlockStatus::Finished(outcome) => Some(outcome),
            BlockStatus::Running => None,
        }
    }

    pub fn suggestion(&self, input: &str) -> Option<&str> {
        if input.is_empty() {
            return None;
        }
        self.blocks
            .iter()
            .rev()
            .map(|block| block.command.as_str())
            .find(|command| command.len() > input.len() && command.starts_with(input))
    }

    pub fn apply(&mut self, mark: ShellMark, now: Instant) -> Option<BlockChange> {
        match mark {
            ShellMark::PromptStart => self.finish(None, now).map(BlockChange::CommandFinished),
            ShellMark::CommandStart => Some(BlockChange::InputStarted(self.start_input())),
            ShellMark::CommandExecuted { command } => {
                Some(BlockChange::CommandStarted(self.start_command(command, now)))
            }
            ShellMark::CommandFinished { exit_code } => self.finish(exit_code, now).map(BlockChange::CommandFinished),
        }
    }

    fn allocate(&mut self) -> BlockId {
        self.next_id = self.next_id.wrapping_add(1);
        BlockId(self.next_id)
    }

    fn start_input(&mut self) -> BlockId {
        let id = self.allocate();
        self.phase = ShellPhase::Editing(id);
        id
    }

    fn start_command(&mut self, command: Option<String>, now: Instant) -> BlockId {
        let id = match self.phase {
            ShellPhase::Editing(id) => id,
            _ => self.allocate(),
        };
        if self.blocks.len() >= BLOCK_LIMIT {
            self.blocks.pop_front();
        }
        let command = command.unwrap_or_default();
        self.blocks.push_back(CommandBlock { id, command, status: BlockStatus::Running, started: now });
        self.phase = ShellPhase::Running(id);
        id
    }

    fn finish(&mut self, exit_code: Option<i32>, now: Instant) -> Option<CommandBlock> {
        let running = match self.phase {
            ShellPhase::Running(id) => Some(id),
            _ => None,
        };
        self.phase = ShellPhase::Prompt;
        let block = self.blocks.iter_mut().rev().find(|block| Some(block.id) == running)?;
        let duration = now.saturating_duration_since(block.started);
        block.status = BlockStatus::Finished(BlockOutcome { exit_code, duration });
        Some(block.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn executed(command: &str) -> ShellMark {
        ShellMark::CommandExecuted { command: Some(command.into()) }
    }

    fn finished(code: i32) -> ShellMark {
        ShellMark::CommandFinished { exit_code: Some(code) }
    }

    fn run(blocks: &mut CommandBlocks, command: &str, code: i32, start: Instant, millis: u64) -> Option<BlockChange> {
        blocks.apply(ShellMark::PromptStart, start);
        blocks.apply(ShellMark::CommandStart, start);
        blocks.apply(executed(command), start);
        blocks.apply(finished(code), start + Duration::from_millis(millis))
    }

    #[test]
    fn a_full_cycle_records_exit_code_and_duration() {
        let mut blocks = CommandBlocks::default();
        let start = Instant::now();
        let Some(BlockChange::CommandFinished(block)) = run(&mut blocks, "false", 1, start, 2100) else {
            panic!("expected a finished block");
        };
        assert_eq!(block.command, "false");
        let outcome = BlockOutcome { exit_code: Some(1), duration: Duration::from_millis(2100) };
        assert_eq!(block.status, BlockStatus::Finished(outcome));
        assert!(outcome.is_failure());
        assert_eq!(blocks.phase(), ShellPhase::Prompt);
        assert_eq!(blocks.last_outcome(), Some(outcome));
    }

    #[test]
    fn input_and_command_share_one_block_id() {
        let mut blocks = CommandBlocks::default();
        let now = Instant::now();
        let Some(BlockChange::InputStarted(input)) = blocks.apply(ShellMark::CommandStart, now) else {
            panic!("expected input to start");
        };
        assert_eq!(blocks.phase(), ShellPhase::Editing(input));
        assert_eq!(blocks.apply(executed("ls"), now), Some(BlockChange::CommandStarted(input)));
        assert_eq!(blocks.phase(), ShellPhase::Running(input));
        assert_eq!(blocks.block(input).map(|block| block.status), Some(BlockStatus::Running));
    }

    #[test]
    fn finish_without_a_running_command_is_ignored() {
        let mut blocks = CommandBlocks::default();
        assert_eq!(blocks.apply(finished(0), Instant::now()), None);
        blocks.apply(ShellMark::CommandStart, Instant::now());
        assert_eq!(blocks.apply(finished(130), Instant::now()), None);
        assert_eq!(blocks.last_outcome(), None);
    }

    #[test]
    fn a_new_prompt_closes_a_command_whose_finish_mark_was_lost() {
        let mut blocks = CommandBlocks::default();
        let now = Instant::now();
        blocks.apply(executed("vim"), now);
        let Some(BlockChange::CommandFinished(block)) = blocks.apply(ShellMark::PromptStart, now) else {
            panic!("expected the dangling command to finish");
        };
        assert_eq!(block.status, BlockStatus::Finished(BlockOutcome { exit_code: None, duration: Duration::ZERO }));
    }

    #[test]
    fn interrupts_are_not_failures() {
        let interrupted = BlockOutcome { exit_code: Some(130), duration: Duration::ZERO };
        assert!(interrupted.is_interrupted() && !interrupted.is_failure());
        let unknown = BlockOutcome { exit_code: None, duration: Duration::ZERO };
        assert!(!unknown.is_failure());
    }

    #[test]
    fn suggestion_prefers_the_most_recent_longer_prefix_match() {
        let mut blocks = CommandBlocks::default();
        let now = Instant::now();
        for command in ["cargo build", "cargo test", "git status", "cargo test --workspace"] {
            run(&mut blocks, command, 0, now, 0);
        }
        assert_eq!(blocks.suggestion("cargo t"), Some("cargo test --workspace"));
        assert_eq!(blocks.suggestion("cargo b"), Some("cargo build"));
        assert_eq!(blocks.suggestion("git status"), None);
        assert_eq!(blocks.suggestion(""), None);
        assert_eq!(blocks.suggestion("npm"), None);
    }

    #[test]
    fn history_is_bounded() {
        let mut blocks = CommandBlocks::default();
        let now = Instant::now();
        for index in 0..BLOCK_LIMIT + 5 {
            run(&mut blocks, &format!("echo {index}"), 0, now, 0);
        }
        assert_eq!(blocks.blocks.len(), BLOCK_LIMIT);
        assert_eq!(blocks.suggestion("echo 50"), Some("echo 504"));
    }
}
