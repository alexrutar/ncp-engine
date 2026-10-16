use crate::Config;
use crate::chars::{Char, CharClass};
use crate::matrix::MatcherDataView;
use crate::score::{
    BONUS_BOUNDARY, BONUS_CAMEL123, BONUS_CONSECUTIVE, BONUS_FIRST_CHAR_MULTIPLIER,
    PENALTY_GAP_EXTENSION, PENALTY_GAP_START, SCORE_MATCH, prefix_bonus,
};

// A gap forgets the preceding run's bonus. A contiguous extension can inherit
// one of these five values (including both default and path configurations).
const BONUSES: [u16; 5] = [
    BONUS_CONSECUTIVE,
    BONUS_CAMEL123,
    BONUS_BOUNDARY,
    BONUS_BOUNDARY + 1,
    BONUS_BOUNDARY + 2,
];
const GAP: u8 = 5;
const FLOOR: u8 = 6;

#[derive(Clone, Copy, Default)]
pub(super) struct Continuations([u16; 5]);

impl Continuations {
    #[inline]
    fn best(self) -> (u16, u8) {
        let mut best = (0, 0);
        for (lane, score) in self.0.into_iter().enumerate() {
            if score >= best.0 {
                best = (score, lane as u8);
            }
        }
        best
    }
}

#[inline]
fn bonus_lane(bonus: u16) -> usize {
    match bonus {
        0..=BONUS_CONSECUTIVE => 0,
        BONUS_CAMEL123 => 1,
        BONUS_BOUNDARY => 2,
        9 => 3,
        10 => 4,
        _ => unreachable!("unsupported boundary bonus"),
    }
}

impl<H: Char> MatcherDataView<'_, H, Continuations> {
    pub(super) fn find_prefixed<const INDICES: bool, N: Char>(
        &mut self,
        needle: &[N],
        prev_class: CharClass,
        config: &Config,
        start: usize,
        indices: &mut Vec<u32>,
    ) -> Option<u16>
    where
        H: PartialEq<N>,
    {
        if !self.setup(needle, prev_class, config) {
            return None;
        }
        let width = self.current_row.len();
        let floor = prefix_bonus(start);
        for (col, cell) in self.current_row.iter_mut().enumerate() {
            *cell = Continuations::default();
            if self.haystack[col] == needle[0] {
                let bonus = self.bonus[col] as u16;
                cell.0[bonus_lane(bonus)] =
                    SCORE_MATCH + BONUS_FIRST_CHAR_MULTIPLIER * bonus + prefix_bonus(start + col);
            }
            if INDICES {
                self.matrix_cells[col].0 = 0;
            }
        }

        for (row, &c) in needle.iter().enumerate().skip(1) {
            let row_start = self.row_offs[row - 1] as usize + 1 - row;
            let mut gap_score = 0u16;
            let mut previous_match = (0u16, 0u8);
            for col in row_start..width {
                let adjacent = self.current_row[col];
                let mut gap_source = GAP;
                let open = previous_match.0.saturating_sub(PENALTY_GAP_START);
                gap_score = gap_score.saturating_sub(PENALTY_GAP_EXTENSION);
                if open > gap_score {
                    gap_score = open;
                    gap_source = previous_match.1;
                }
                // max_A(max(T_A - penalty, P_A)) =
                // max(max_A(T_A) - penalty, max_A(P_A)). Every reachable gap
                // admits the earliest first match, so max_A(P_A) is constant.
                if gap_score <= floor {
                    gap_score = floor;
                    gap_source = FLOOR;
                }

                let mut next = Continuations::default();
                let mut source = GAP;
                let pos = col + row;
                if self.haystack[pos] == c {
                    let bonus = self.bonus[pos] as u16;
                    let local = bonus_lane(bonus);
                    for (lane, score) in adjacent.0.into_iter().enumerate() {
                        if score == 0 {
                            continue;
                        }
                        let target = if bonus >= BONUS_BOUNDARY {
                            lane.max(local)
                        } else {
                            lane
                        };
                        let score = score + SCORE_MATCH + BONUSES[target].max(bonus);
                        if score >= next.0[target] {
                            next.0[target] = score;
                            if target == local {
                                source = lane as u8;
                            }
                        }
                    }
                    // Before the first reachable match there is no gap state;
                    // the protected floor must never manufacture reachability.
                    let score = gap_score + SCORE_MATCH + bonus;
                    if col > row_start && score >= next.0[local] {
                        next.0[local] = score;
                        source = GAP;
                    }
                }
                if INDICES {
                    // Only the local-bonus lane can have multiple predecessors;
                    // all other lanes simply continue themselves. Three bits
                    // for that predecessor and three for the gap fit one byte.
                    self.matrix_cells[(row - 1) * width + col].0 |= gap_source << 3;
                    self.matrix_cells[row * width + col].0 = source;
                }
                previous_match = adjacent.best();
                self.current_row[col] = next;
            }
        }

        let row = needle.len() - 1;
        let row_start = self.row_offs[row] as usize - row;
        let (col, (score, lane)) = self.current_row[row_start..]
            .iter()
            .enumerate()
            .map(|(col, cell)| (col + row_start, cell.best()))
            .max_by_key(|&(_, (score, _))| score)
            .unwrap();
        if INDICES {
            self.traceback(row, col + row, lane, start as u32, indices);
        }
        Some(score)
    }

    fn traceback(
        &self,
        mut row: usize,
        mut pos: usize,
        mut lane: u8,
        start: u32,
        indices: &mut Vec<u32>,
    ) {
        let offset = indices.len();
        indices.resize(offset + row + 1, 0);
        let indices = &mut indices[offset..];
        let width = self.current_row.len();
        loop {
            let trace = self.matrix_cells[row * width + pos - row].0;
            if lane == GAP {
                lane = trace >> 3;
                if lane == FLOOR {
                    for (index, &earliest) in indices[..=row].iter_mut().zip(self.row_offs.iter()) {
                        *index = start + earliest as u32;
                    }
                    break;
                }
            } else {
                indices[row] = start + pos as u32;
                if row == 0 {
                    break;
                }
                if lane as usize == bonus_lane(self.bonus[pos] as u16) {
                    lane = trace & 7;
                }
                row -= 1;
            }
            pos -= 1;
        }
    }
}
