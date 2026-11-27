//! Frames prepared by the Composition viewer while it is idle.
use crate::session::PreviewRange;
use bae_model::{FrameRate, Time, TimeRange};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Order {
    FromStart,
    FromCurrent,
    AroundCurrent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    first: i64,
    last: i64,
    current: i64,
    order: Order,
}

impl Plan {
    pub(crate) fn from_prefs(
        prefs: &bae_prefs::Preferences,
        info: &bae_motion::query::CompInfo,
        time: Time,
    ) -> Option<Self> {
        let range = match prefs.str("previews.cache_range") {
            "entire_duration" => PreviewRange::EntireDuration,
            "work_area" => PreviewRange::WorkArea,
            _ => PreviewRange::WorkAreaExtended,
        }
        .bounds(info, time);
        let order = match prefs.str("previews.cache_frames_order") {
            "from_start_of_range" => Order::FromStart,
            "from_current_time" => Order::FromCurrent,
            _ => Order::AroundCurrent,
        };
        Self::new(range, time, info.frame_rate, order)
    }

    fn new(range: TimeRange, time: Time, rate: FrameRate, order: Order) -> Option<Self> {
        if range.start >= range.end {
            return None;
        }
        let first = rate.time_to_frame(range.start);
        let first = first + i64::from(rate.frame_to_time(first) < range.start);
        let last = rate.time_to_frame(range.end - Time(1));
        if first > last {
            return None;
        }
        Some(Self {
            first,
            last,
            current: rate.time_to_frame(time).clamp(first, last),
            order,
        })
    }

    /// One frame by ordinal, so long compositions never allocate a list of times.
    pub(super) fn frame(self, n: i64) -> Option<i64> {
        let count = self.last - self.first + 1;
        if n >= count {
            return None;
        }
        Some(match self.order {
            Order::FromStart => self.first + n,
            Order::FromCurrent => self.first + (self.current - self.first + n) % count,
            Order::AroundCurrent => {
                let left = self.current - self.first;
                let right = self.last - self.current;
                let pairs = left.min(right);
                if n <= pairs * 2 {
                    if n % 2 == 1 {
                        self.current + (n + 1) / 2
                    } else {
                        self.current - n / 2
                    }
                } else if right > left {
                    self.current + n - pairs
                } else {
                    self.current - n + pairs
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_visits_every_frame_once_within_the_half_open_range() {
        let plan = Plan {
            first: 2,
            current: 4,
            last: 7,
            order: Order::AroundCurrent,
        };
        let frames = |p: Plan| (0..).map_while(|n| p.frame(n)).collect::<Vec<_>>();
        assert_eq!(frames(plan), [4, 5, 3, 6, 2, 7]);
        assert_eq!(
            frames(Plan {
                order: Order::FromCurrent,
                ..plan
            }),
            [4, 5, 6, 7, 2, 3]
        );
        assert_eq!(
            frames(Plan {
                order: Order::FromStart,
                ..plan
            }),
            [2, 3, 4, 5, 6, 7]
        );
        assert_eq!(frames(Plan { current: 7, ..plan }), [7, 6, 5, 4, 3, 2]);
    }
}
