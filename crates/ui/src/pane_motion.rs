#![deny(warnings)]
#![forbid(unsafe_code)]

use crate::pane_layout::{Island, PaneLayout, Rect};
use domain::{LayoutNode, PanelId, TabId};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const MILLIS_PER_SECOND: f32 = 1000.0;
const HIDDEN: f32 = 0.0;
const SHOWN: f32 = 1.0;

pub fn ease_out_cubic(t: f32) -> f32 {
    let remaining = 1.0 - t.clamp(0.0, 1.0);
    1.0 - remaining * remaining * remaining
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

pub fn tween(from: Rect, to: Rect, t: f32) -> Rect {
    Rect::new(
        lerp(from.x, to.x, t),
        lerp(from.y, to.y, t),
        lerp(from.width, to.width, t),
        lerp(from.height, to.height, t),
    )
}

pub fn progress(started: Instant, now: Instant, duration: Duration) -> f32 {
    if duration.is_zero() {
        return 1.0;
    }
    (now.saturating_duration_since(started).as_secs_f32() / duration.as_secs_f32()).min(1.0)
}

pub fn duration_from_ms(animation_ms: f32) -> Duration {
    Duration::from_secs_f32(animation_ms.max(0.0) / MILLIS_PER_SECOND)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IslandFrame {
    pub rect: Rect,
    pub opacity: f32,
}

impl IslandFrame {
    fn shown(rect: Rect) -> Self {
        Self { rect, opacity: SHOWN }
    }

    fn hidden(rect: Rect) -> Self {
        Self { rect, opacity: HIDDEN }
    }
}

#[derive(Debug, Clone, Copy)]
struct Track {
    from: IslandFrame,
    to: IslandFrame,
    exit: Rect,
    started: Instant,
}

impl Track {
    fn settled(island: &Island, now: Instant) -> Self {
        let frame = IslandFrame::shown(island.rect);
        Self { from: frame, to: frame, exit: island.collapsed, started: now }
    }

    fn at(&self, now: Instant, duration: Duration) -> IslandFrame {
        let t = ease_out_cubic(progress(self.started, now, duration));
        let opacity = lerp(self.from.opacity, self.to.opacity, t);
        IslandFrame { rect: tween(self.from.rect, self.to.rect, t), opacity }
    }

    fn is_running(&self, now: Instant, duration: Duration) -> bool {
        progress(self.started, now, duration) < 1.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub tab: TabId,
    pub tree: LayoutNode,
    pub is_fresh_tab: bool,
    pub is_live_resize: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    Cut,
    FadeIn,
    Follow,
    Glide,
}

pub fn transition_for(previous: Option<&Scene>, next: &Scene) -> Transition {
    let is_same_tab = previous.is_some_and(|scene| scene.tab == next.tab);
    if !is_same_tab {
        return match next.is_fresh_tab {
            true => Transition::FadeIn,
            false => Transition::Cut,
        };
    }
    let is_same_tree = previous.is_some_and(|scene| scene.tree == next.tree);
    if is_same_tree || next.is_live_resize {
        return Transition::Follow;
    }
    Transition::Glide
}

#[derive(Debug)]
pub struct PaneMotion {
    duration: Duration,
    islands: HashMap<PanelId, Track>,
    ghosts: Vec<(PanelId, Track)>,
}

impl PaneMotion {
    pub fn new(duration: Duration) -> Self {
        Self { duration, islands: HashMap::new(), ghosts: Vec::new() }
    }

    pub fn apply(&mut self, transition: Transition, layout: &PaneLayout, now: Instant) {
        match transition {
            Transition::Cut => self.cut_to(layout, now),
            Transition::FadeIn => self.fade_in_to(layout, now),
            Transition::Follow => self.follow(layout, now),
            Transition::Glide => self.glide_to(layout, now),
        }
    }

    pub fn frame(&self, panel: PanelId, now: Instant) -> Option<IslandFrame> {
        self.islands.get(&panel).map(|track| track.at(now, self.duration))
    }

    pub fn ghosts(&self, now: Instant) -> Vec<(PanelId, IslandFrame)> {
        self.ghosts
            .iter()
            .filter(|(_, track)| track.is_running(now, self.duration))
            .map(|(panel, track)| (*panel, track.at(now, self.duration)))
            .collect()
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        let mut tracks = self.islands.values().chain(self.ghosts.iter().map(|(_, track)| track));
        tracks.any(|track| track.is_running(now, self.duration))
    }

    pub fn prune(&mut self, now: Instant) {
        let duration = self.duration;
        self.ghosts.retain(|(_, track)| track.is_running(now, duration));
    }

    fn cut_to(&mut self, layout: &PaneLayout, now: Instant) {
        self.ghosts.clear();
        self.islands = layout.islands.iter().map(|island| (island.panel, Track::settled(island, now))).collect();
    }

    fn fade_in_to(&mut self, layout: &PaneLayout, now: Instant) {
        self.ghosts.clear();
        self.islands = layout
            .islands
            .iter()
            .map(|island| {
                let track = Track {
                    from: IslandFrame::hidden(island.rect),
                    to: IslandFrame::shown(island.rect),
                    exit: island.collapsed,
                    started: now,
                };
                (island.panel, track)
            })
            .collect();
    }

    fn follow(&mut self, layout: &PaneLayout, now: Instant) {
        for island in &layout.islands {
            let target = IslandFrame::shown(island.rect);
            let is_on_target = self.islands.get(&island.panel).is_some_and(|track| track.to == target);
            if !is_on_target {
                self.islands.insert(island.panel, Track::settled(island, now));
            }
        }
        self.islands.retain(|panel, _| layout.islands.iter().any(|island| island.panel == *panel));
    }

    fn glide_to(&mut self, layout: &PaneLayout, now: Instant) {
        let mut previous = std::mem::take(&mut self.islands);
        for island in &layout.islands {
            let target = IslandFrame::shown(island.rect);
            let kept = previous.remove(&island.panel);
            let track = match kept {
                Some(track) if track.to == target => Track { exit: island.collapsed, ..track },
                Some(track) => {
                    Track { from: track.at(now, self.duration), to: target, exit: island.collapsed, started: now }
                }
                None => {
                    let from = self.take_ghost(island.panel, now).unwrap_or(IslandFrame::hidden(island.collapsed));
                    Track { from, to: target, exit: island.collapsed, started: now }
                }
            };
            self.islands.insert(island.panel, track);
        }
        for (panel, track) in previous {
            let from = track.at(now, self.duration);
            let exit = Track { from, to: IslandFrame::hidden(track.exit), exit: track.exit, started: now };
            self.ghosts.push((panel, exit));
        }
    }

    fn take_ghost(&mut self, panel: PanelId, now: Instant) -> Option<IslandFrame> {
        let index = self.ghosts.iter().position(|(ghost, _)| *ghost == panel)?;
        let (_, track) = self.ghosts.remove(index);
        Some(track.at(now, self.duration))
    }
}
