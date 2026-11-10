use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};

use tokio::sync::Notify;

use super::frame::{Frame, MapTransfer};
use crate::protocol::{Outcome, Refused};

pub(crate) const PEER_BACKLOG_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Normal,
    Welcome,
    Resync,
    Closing,
}

pub(crate) enum Body {
    Frame(Frame),
    Map(MapTransfer),
}

pub(crate) struct Outgoing {
    pub(crate) body: Body,
    pub(crate) kind: Kind,
    pub(crate) outcomes: Vec<Outcome>,
}

impl Outgoing {
    pub(crate) fn new(frame: Frame, kind: Kind) -> Self {
        Self { body: Body::Frame(frame), kind, outcomes: Vec::new() }
    }

    pub(crate) fn map(transfer: MapTransfer, kind: Kind, outcomes: Vec<Outcome>) -> Self {
        Self { body: Body::Map(transfer), kind, outcomes }
    }

    pub(crate) fn wire_bytes(&self) -> u64 {
        match &self.body {
            Body::Frame(frame) => frame.wire_bytes(),
            Body::Map(_) => 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn frames(self) -> Vec<Frame> {
        match self.body {
            Body::Frame(frame) => vec![frame],
            Body::Map(transfer) => transfer.frames().expect("map transfer frames"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Pushed {
    Queued,
    Behind(Vec<Outcome>),
    Closed,
}

#[derive(Default)]
struct State {
    items: VecDeque<Outgoing>,
    backlog: usize,
    refused: Option<Refused>,
    detached: bool,
}

pub(crate) struct PeerQueue {
    state: Mutex<State>,
    ready: Notify,
    cancel: Notify,
    budget: usize,
}

impl PeerQueue {
    pub(crate) fn new(budget: usize) -> Self {
        Self { state: Mutex::new(State::default()), ready: Notify::new(), cancel: Notify::new(), budget }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn push(&self, item: Outgoing) -> Pushed {
        let mut state = self.lock();
        if state.refused.is_some() || state.detached {
            return Pushed::Closed;
        }
        let size = item.wire_bytes() as usize;
        if item.kind == Kind::Normal && state.backlog > 0 && state.backlog + size > self.budget {
            let mut outcomes = Vec::new();
            let mut kept = VecDeque::with_capacity(state.items.len());
            for queued in std::mem::take(&mut state.items) {
                if matches!(queued.kind, Kind::Normal | Kind::Resync) {
                    outcomes.extend(queued.outcomes);
                } else {
                    kept.push_back(queued);
                }
            }
            state.items = kept;
            state.backlog = 0;
            outcomes.extend(item.outcomes);
            return Pushed::Behind(outcomes);
        }
        if item.kind == Kind::Normal {
            state.backlog += size;
        }
        state.items.push_back(item);
        drop(state);
        self.ready.notify_one();
        Pushed::Queued
    }

    pub(crate) fn refuse(&self, refused: Refused, frame: Option<Frame>) {
        let mut state = self.lock();
        if state.refused.is_some() {
            return;
        }
        state.items.clear();
        state.backlog = 0;
        state.refused = Some(refused);
        if let Some(frame) = frame {
            state.items.push_back(Outgoing::new(frame, Kind::Closing));
        }
        drop(state);
        self.ready.notify_one();
        self.cancel.notify_one();
    }

    pub(crate) fn detach(&self) {
        self.lock().detached = true;
        self.ready.notify_one();
    }

    pub(crate) fn refused(&self) -> Option<Refused> {
        self.lock().refused.clone()
    }

    pub(crate) fn try_pop(&self) -> Option<Outgoing> {
        let mut state = self.lock();
        let item = state.items.pop_front()?;
        if item.kind == Kind::Normal {
            state.backlog -= item.wire_bytes() as usize;
        }
        Some(item)
    }

    pub(crate) async fn pop(&self) -> Option<Outgoing> {
        loop {
            let notified = self.ready.notified();
            if let Some(item) = self.try_pop() {
                return Some(item);
            }
            {
                let state = self.lock();
                if state.detached || (state.refused.is_some() && state.items.is_empty()) {
                    return None;
                }
            }
            notified.await;
        }
    }

    pub(crate) async fn cancelled(&self) {
        loop {
            self.cancel.notified().await;
            if self.lock().refused.is_some() {
                return;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn backlog(&self) -> usize {
        self.lock().backlog
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{EditRefusal, Refusal, Rejection};

    fn frame(bytes: usize) -> Frame {
        Frame::from_parts(vec![bytes::Bytes::from(vec![b' '; bytes])]).unwrap()
    }

    fn outcome(sequence: u64) -> Outcome {
        Outcome { sequence, rejection: None }
    }

    #[test]
    fn a_peer_that_falls_behind_keeps_its_welcome_and_collects_its_own_outcomes() {
        let queue = PeerQueue::new(1000);
        assert_eq!(queue.push(Outgoing::new(frame(5000), Kind::Welcome)), Pushed::Queued);
        for sequence in 0..3 {
            let item =
                Outgoing { body: Body::Frame(frame(296)), kind: Kind::Normal, outcomes: vec![outcome(sequence)] };
            assert_eq!(queue.push(item), Pushed::Queued);
        }
        assert_eq!(queue.backlog(), 900);
        let rejected = Outcome { sequence: 3, rejection: Some(Rejection::new(EditRefusal::Selected, "solid:4")) };
        let last = Outgoing { body: Body::Frame(frame(296)), kind: Kind::Normal, outcomes: vec![rejected.clone()] };
        let Pushed::Behind(outcomes) = queue.push(last) else { panic!("behind") };
        assert_eq!(outcomes, vec![outcome(0), outcome(1), outcome(2), rejected]);
        assert_eq!(queue.backlog(), 0);
        let resync = Outgoing { body: Body::Frame(frame(9000)), kind: Kind::Resync, outcomes: outcomes.clone() };
        assert_eq!(queue.push(resync), Pushed::Queued);
        assert_eq!(queue.push(Outgoing::new(frame(996), Kind::Normal)), Pushed::Queued);
        let Pushed::Behind(again) = queue.push(Outgoing::new(frame(10), Kind::Normal)) else { panic!("behind") };
        assert_eq!(again, outcomes, "a replaced resync hands its outcomes to the next one");
        assert_eq!(queue.try_pop().map(|item| item.kind), Some(Kind::Welcome));
        assert!(queue.try_pop().is_none());
    }

    #[test]
    fn a_single_frame_larger_than_the_budget_is_still_queued_when_nothing_waits() {
        let queue = PeerQueue::new(100);
        assert_eq!(queue.push(Outgoing::new(frame(5000), Kind::Normal)), Pushed::Queued);
        assert_eq!(queue.try_pop().map(|item| item.wire_bytes()), Some(5004));
        assert_eq!(queue.backlog(), 0);
    }

    #[tokio::test]
    async fn refusals_replace_the_queue_and_cancel_a_write_in_progress() {
        let queue = std::sync::Arc::new(PeerQueue::new(1000));
        queue.push(Outgoing::new(frame(50), Kind::Normal));
        queue.push(Outgoing::new(frame(60), Kind::Normal));
        let waiting = {
            let queue = queue.clone();
            tokio::spawn(async move { queue.cancelled().await })
        };
        queue.refuse(Refused::new(Refusal::Removed), Some(frame(7)));
        tokio::time::timeout(std::time::Duration::from_secs(5), waiting).await.unwrap().unwrap();
        assert_eq!(queue.push(Outgoing::new(frame(8), Kind::Normal)), Pushed::Closed);
        let closing = queue.pop().await.unwrap();
        assert_eq!((closing.kind, closing.wire_bytes()), (Kind::Closing, 11));
        assert!(queue.pop().await.is_none());
        assert_eq!(queue.refused().map(|refused| refused.refusal), Some(Refusal::Removed));
        let detached = PeerQueue::new(10);
        detached.detach();
        assert!(detached.pop().await.is_none());
    }
}
