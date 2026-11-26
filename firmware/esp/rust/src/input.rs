//! タッチ点の追跡。GT911 の生の点（物理座標）を指ごとの down/move/up にし、
//! 上端からの下スワイプ（システムジェスチャ = ランチャーへ）を横取りする。
use alloc::vec::Vec;

use wasmos_host::bindings::wasmos::os::types::{Event, Touch, TouchPhase};

use crate::platform::from_panel;

const GESTURE_EDGE: i32 = 24;
const GESTURE_DISTANCE: i32 = 160;
/// 前回の点と同じ指とみなす距離
const MATCH_DIST: i32 = 120;

struct Finger {
    id: u8,
    x: i32,
    y: i32,
    /// 上端から始まった指（アプリに渡さない）。開始 y
    gesture: Option<i32>,
}

pub struct Tracker {
    fingers: Vec<Finger>,
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            fingers: Vec::new(),
        }
    }

    /// (アプリに送るイベント, ホームジェスチャが成立したか)
    pub fn update(&mut self, raw: &[(u16, u16)]) -> (Vec<Event>, bool) {
        let pts: Vec<(i32, i32)> = raw
            .iter()
            .map(|&(px, py)| from_panel(px as i32, py as i32))
            .collect();
        let mut touches = Vec::new();
        let mut home = false;
        let mut used = alloc::vec![false; pts.len()];
        let mut next = Vec::new();

        // 既存の指に一番近い点を割り当てる
        for f in self.fingers.drain(..) {
            let best = pts
                .iter()
                .enumerate()
                .filter(|(i, _)| !used[*i])
                .map(|(i, p)| (i, (p.0 - f.x).abs() + (p.1 - f.y).abs()))
                .filter(|(_, d)| *d < MATCH_DIST)
                .min_by_key(|(_, d)| *d);
            match best {
                Some((i, _)) => {
                    used[i] = true;
                    let (x, y) = pts[i];
                    match f.gesture {
                        Some(y0) => {
                            if y - y0 > GESTURE_DISTANCE {
                                home = true;
                            }
                        }
                        None if (x, y) != (f.x, f.y) => touches.push(Touch {
                            id: f.id,
                            phase: TouchPhase::Move,
                            x,
                            y,
                        }),
                        None => {}
                    }
                    next.push(Finger { x, y, ..f });
                }
                None => {
                    if f.gesture.is_none() {
                        touches.push(Touch {
                            id: f.id,
                            phase: TouchPhase::Up,
                            x: f.x,
                            y: f.y,
                        });
                    }
                }
            }
        }
        // 新しい指
        for (i, &(x, y)) in pts.iter().enumerate() {
            if used[i] {
                continue;
            }
            let id = (0u8..)
                .find(|id| !next.iter().any(|f| f.id == *id))
                .unwrap_or(0);
            let gesture = (y < GESTURE_EDGE).then_some(y);
            if gesture.is_none() {
                touches.push(Touch {
                    id,
                    phase: TouchPhase::Down,
                    x,
                    y,
                });
            }
            next.push(Finger { id, x, y, gesture });
        }
        if home {
            // ジェスチャは 1 回だけ
            for f in next.iter_mut() {
                if f.gesture.is_some() {
                    f.gesture = Some(i32::MAX / 2);
                }
            }
        }
        self.fingers = next;
        let events = if touches.is_empty() {
            Vec::new()
        } else {
            alloc::vec![Event::Touch(touches)]
        };
        (events, home)
    }
}
