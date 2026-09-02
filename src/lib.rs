//! # Nucleo-picker-engine
//!
//! The `ncp-engine` crate is a fork of the [nucleo](https://docs.rs/nucleo) crate.
//! It is not recommended for general use. This fork mainly exists to meet the specific
//! requirements of [`nucleo-picker`](https://docs.rs/nucleo-picker).
//!
//! `ncp-engine` implements a high level matcher API that provides a performant
//! parallel matcher worker threadpool. It is designed to allow integrating a `fzf`-like
//! fuzzy matcher into a TUI application.
//!
//! For a fully-featured TUI implementation, see [nucleo-picker](htpps://docs.rs/nucleo-picker).
//!
//! Matching runs in a background threadpool while providing a snapshot of the last
//! complete match on request. That means the matcher can update the results live while
//! the user is typing, while never blocking the main UI thread (beyond a user provided
//! timeout). Nucleo also supports fully concurrent lock-free (and wait-free) streaming
//! of input items.
//!
//! The [`Nucleo`] struct serves as the main API entrypoint for this crate.

#![deny(missing_docs)]

use std::ops::{Bound, RangeBounds};
use std::sync::Arc;
use std::sync::atomic::{self, AtomicBool, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use rayon::ThreadPool;

use crate::pattern::MultiPattern;
pub use crate::worker::MatchListConfig;
use crate::worker::Worker;
pub use ncp_matcher::{Config, Matcher, Utf32Str, Utf32String, chars};

mod boxcar;
mod par_sort;
pub mod pattern;
mod worker;

#[cfg(test)]
mod tests;

/// A match candidate stored in a [`Nucleo`] worker.
pub struct Item<'a, T> {
    /// A reference to the underlying item provided to the matcher.
    pub data: &'a T,
    /// The representation of the data within the matcher.
    pub matcher_columns: &'a [Utf32String],
}

/// A detached handle to a match candidate.
///
/// Unlike an [`Item`], which has a lifetime tied to the [`Snapshot`] or [`Injector`] from which it was
/// created, a [`DetachedItem`] owns a handle to the underlying data and will persist even if the original
/// [`Nucleo`] matcher has been dropped.
///
/// Similarly to an [injector](Injector), holding this handle will prevent the underlying data
/// from being dropped. This handle is internally reference counted and can be cloned cheaply.
///
/// This handle implements [`PartialEq`] regardless of the value `T`. Equality is tested by checking that
/// both items originate from the same matcher and that the internal indices are the same.
pub struct DetachedItem<T> {
    items: Arc<boxcar::Vec<T>>,
    // this index is guaranteed to be valid for self.items
    idx: u32,
}

impl<T> DetachedItem<T> {
    unsafe fn new(items: &Arc<boxcar::Vec<T>>, idx: u32) -> Self {
        Self {
            items: Arc::clone(items),
            idx,
        }
    }

    /// Get the corresponding item.
    pub fn item(&self) -> Item<'_, T> {
        unsafe { self.items.get_unchecked(self.idx) }
    }

    /// Get the raw underlying index.
    ///
    /// This index is guaranteed to be valid for the snapshot or injector from which this
    /// [`DetachedItem`] was originally constructed.
    pub fn idx(&self) -> u32 {
        self.idx
    }
}

impl<T> PartialEq for DetachedItem<T> {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx && Arc::ptr_eq(&self.items, &other.items)
    }
}

impl<T> Clone for DetachedItem<T> {
    fn clone(&self) -> Self {
        Self {
            items: Arc::clone(&self.items),
            idx: self.idx,
        }
    }
}

/// A handle that allows adding new items to a [`Nucleo`] worker.
///
/// An `Injector` is internally reference counted and can be cheaply
/// cloned and sent across threads.
pub struct Injector<T> {
    items: Arc<boxcar::Vec<T>>,
    notify: Arc<dyn Fn() + Sync + Send>,
}

impl<T> Clone for Injector<T> {
    fn clone(&self) -> Self {
        Self {
            items: self.items.clone(),
            notify: self.notify.clone(),
        }
    }
}

impl<T> Injector<T> {
    /// Appends an element to the list of match candidates.
    ///
    /// This function is lock-free and wait-free. The returned `u32` is the internal index which
    /// has been assigned to the provided value and is guaranteed to be valid unless
    /// [`Nucleo::restart`] has been called.
    ///
    /// The `fill_columns` closure is called to generate the representation of the pushed value
    /// within the matcher engine. The first argument is a reference to the provided value, and the
    /// second argument is a slice where each entry corresponds to a column within the [`Nucleo`]
    /// instance from which this `Injector` was created.
    ///
    /// ## Example
    /// If the matcher has exactly one column and the item type `T` is a `String`, an appropriate
    /// `fill_columns` closure might look like
    /// ```
    /// # use ncp_engine::Utf32String;
    /// let fill_columns = |s: &String, cols: &mut [Utf32String]| {
    ///      cols[0] = (&**s).into();
    /// };
    /// ```
    pub fn push(&self, value: T, fill_columns: impl FnOnce(&T, &mut [Utf32String])) -> u32 {
        let idx = self.items.push(value, fill_columns);
        (self.notify)();
        idx
    }

    /// Appends multiple elements to the list of matched items. This function is lock-free
    /// and wait-free.
    ///
    /// You should favor this function over `push` if at least one of the following is true:
    /// - The number of items you're adding can be computed beforehand and is typically larger
    ///   than 1k
    /// - You're able to batch incoming items
    /// - You're adding items from multiple threads concurrently (this function results in less
    ///   contention)
    pub fn extend<I>(&self, values: I, fill_columns: impl Fn(&T, &mut [Utf32String]))
    where
        I: IntoIterator<Item = T>,
    {
        self.items.extend(values, fill_columns);
        (self.notify)();
    }

    /// Returns the total number of items injected in the matcher.
    ///
    /// This may not match the number of items in the match snapshot if the matcher
    /// is still running.
    pub fn injected_items(&self) -> u32 {
        self.items.count()
    }

    /// Returns a reference to the item at the given index.
    #[inline]
    pub fn get_item(&self, index: u32) -> Option<Item<'_, T>> {
        self.items.get(index)
    }

    /// Returns a reference to the item at the given index without checking that the index is valid.
    ///
    /// # Safety
    ///
    /// Item at `index` must be initialized. That means you must have observed
    /// `push` returning this value or `get` returning `Some` for this value.
    /// Just because a later index is initialized doesn't mean that this index
    /// is initialized
    #[inline]
    pub unsafe fn get_item_unchecked(&self, index: u32) -> Item<'_, T> {
        unsafe { self.items.get_unchecked(index) }
    }

    /// Returns the detached item at the given index.
    #[inline]
    pub fn get_detached_item(&self, index: u32) -> Option<DetachedItem<T>> {
        self.items
            .is_valid(index)
            .then(|| unsafe { DetachedItem::new(&self.items, index) })
    }

    /// Returns the detached item at the given index without checking that the index is valid.
    ///
    /// # Safety
    ///
    /// Item at `index` must be initialized. That means you must have observed
    /// `push` returning this value or `get` returning `Some` for this value.
    /// Just because a later index is initialized doesn't mean that this index
    /// is initialized
    #[inline]
    pub unsafe fn get_detached_item_unchecked(&self, index: u32) -> DetachedItem<T> {
        unsafe { DetachedItem::new(&self.items, index) }
    }
}

/// A successful match computed by the [`Nucleo`] match.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct Match {
    /// The score of the match.
    pub score: u32,
    /// The index of the match.
    ///
    /// The index is guaranteed to correspond to a valid item within the matcher and within the
    /// same snapshot. Note that indices are invalidated if the matcher engine has been
    /// [restarted](Nucleo::restart).
    pub idx: u32,
}

/// The status of a [`Nucleo`] worker after a match.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct Status {
    /// Whether the current snapshot has changed.
    pub changed: bool,
    /// Whether the matcher is still processing in the background.
    pub running: bool,
}

/// A representation of the results of a [`Nucleo`] worker after finishing a
/// [`tick`](Nucleo::tick).
pub struct Snapshot<T> {
    item_count: u32,
    matches: Vec<Match>,
    pattern: MultiPattern,
    items: Arc<boxcar::Vec<T>>,
}

impl<T: Sync + Send + 'static> Snapshot<T> {
    fn clear(&mut self, new_items: Arc<boxcar::Vec<T>>) {
        self.item_count = 0;
        self.matches.clear();
        self.items = new_items;
    }

    fn update(&mut self, worker: &Worker<T>) {
        self.item_count = worker.item_count();
        self.pattern.clone_from(&worker.pattern);
        self.matches.clone_from(&worker.matches);
        if !Arc::ptr_eq(&worker.items, &self.items) {
            self.items = worker.items.clone();
        }
    }

    /// Returns that total number of items
    pub fn item_count(&self) -> u32 {
        self.item_count
    }

    /// Returns the pattern which items were matched against
    pub fn pattern(&self) -> &MultiPattern {
        &self.pattern
    }

    /// Returns that number of items that matched the pattern
    pub fn matched_item_count(&self) -> u32 {
        self.matches.len() as u32
    }

    /// Returns an iterator over the items that correspond to a subrange of
    /// all the matches in this snapshot.
    ///
    /// # Panics
    /// Panics if `range` has a range bound that is larger than
    /// the matched item count
    pub fn matched_items(
        &self,
        range: impl RangeBounds<u32>,
    ) -> impl ExactSizeIterator<Item = Item<'_, T>> + DoubleEndedIterator + '_ {
        // TODO: use TAIT
        let start = match range.start_bound() {
            Bound::Included(&start) => start as usize,
            Bound::Excluded(&start) => start as usize + 1,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&end) => end as usize + 1,
            Bound::Excluded(&end) => end as usize,
            Bound::Unbounded => self.matches.len(),
        };
        self.matches[start..end]
            .iter()
            .map(|&m| unsafe { self.items.get_unchecked(m.idx) })
    }

    /// Returns a reference to the item at the given index.
    ///
    /// Returns `None` if the given `index` is not initialized. This function
    /// is only guarteed to return `Some` for item indices that can be found in
    /// the `matches` of this struct. Both smaller and larger indices may return
    /// `None`.
    #[inline]
    pub fn get_item(&self, index: u32) -> Option<Item<'_, T>> {
        self.items.get(index)
    }

    /// Returns a reference to the item at the given index.
    ///
    /// # Safety
    ///
    /// Item at `index` must be initialized. That means you must have observed a
    /// match with the corresponding index in this exact snapshot. Observing
    /// a higher index is not enough as item indices can be non-contigously
    /// initialized
    #[inline]
    pub unsafe fn get_item_unchecked(&self, index: u32) -> Item<'_, T> {
        unsafe { self.items.get_unchecked(index) }
    }

    /// Returns the detached item at the given index.
    #[inline]
    pub fn get_detached_item(&self, index: u32) -> Option<DetachedItem<T>> {
        self.items
            .is_valid(index)
            .then(|| unsafe { DetachedItem::new(&self.items, index) })
    }

    /// Returns the detached item at the given index without checking that the index is valid.
    ///
    /// # Safety
    ///
    /// Item at `index` must be initialized. That means you must have observed
    /// `push` returning this value or `get` returning `Some` for this value.
    /// Just because a later index is initialized doesn't mean that this index
    /// is initialized
    #[inline]
    pub unsafe fn get_detached_item_unchecked(&self, index: u32) -> DetachedItem<T> {
        unsafe { DetachedItem::new(&self.items, index) }
    }

    /// Returns the matches corresponding to this snapshot.
    #[inline]
    pub fn matches(&self) -> &[Match] {
        &self.matches
    }

    /// A convenience function to return the [`Item`] corresponding to the
    /// `n`th match.
    ///
    /// Returns `None` if `n` is greater than or equal to the match count.
    #[inline]
    pub fn get_matched_item(&self, n: u32) -> Option<Item<'_, T>> {
        // SAFETY: A match index is guaranteed to corresponding to a valid global index in this
        // snapshot.
        unsafe { Some(self.get_item_unchecked(self.matches.get(n as usize)?.idx)) }
    }

    /// A convenience function to return the [`DetachedItem`] corresponding to the
    /// `n`th match.
    ///
    /// Returns `None` if `n` is greater than or equal to the match count.
    #[inline]
    pub fn get_matched_detached_item(&self, n: u32) -> Option<DetachedItem<T>> {
        // SAFETY: A match index is guaranteed to corresponding to a valid global index in this
        // snapshot.
        unsafe { Some(self.get_detached_item_unchecked(self.matches.get(n as usize)?.idx)) }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Init,
    /// items have been cleared but snapshot and items are still outdated
    Cleared,
    /// items are fresh
    Fresh,
}

impl State {
    fn matcher_item_refs(self) -> usize {
        match self {
            Self::Cleared => 1,
            Self::Init | Self::Fresh => 2,
        }
    }

    fn canceled(self) -> bool {
        self != Self::Fresh
    }

    fn cleared(self) -> bool {
        self != Self::Fresh
    }
}

/// A high level matcher worker that quickly computes matches in a background
/// threadpool.
///
/// ## Example
/// ```
/// use std::sync::atomic::{AtomicBool, Ordering};
/// use std::sync::Arc;
/// use std::thread;
///
/// use ncp_engine::{Config, Nucleo};
///
/// static NEEDS_UPDATE: AtomicBool = AtomicBool::new(false);
///
/// // initialize a new matcher with default configuration and one column
/// let matcher = Nucleo::new(
///     Config::DEFAULT,
///     Arc::new(|| NEEDS_UPDATE.store(true, Ordering::Relaxed)),
///     None,
///     1
/// );
///
/// // get a handle to add items to the matcher
/// let injector = matcher.injector();
///
/// // add items to the matcher
/// thread::spawn(move || {
///     injector.push("Hello, world!".to_string(), |s, cols| {
///         cols[0] = (&**s).into();
///     });
/// });
/// ```
pub struct Nucleo<T> {
    // the way the API is build we totally don't actually need these to be Arcs
    // but this lets us avoid some unsafe
    canceled: Arc<AtomicBool>,
    should_notify: Arc<AtomicBool>,
    worker: Arc<Mutex<Worker<T>>>,
    pool: ThreadPool,
    state: State,
    items: Arc<boxcar::Vec<T>>,
    notify: Arc<dyn Fn() + Sync + Send>,
    snapshot: Snapshot<T>,
    /// The pattern matched by this matcher.
    ///
    /// To update the match pattern, use [`MultiPattern::reparse`]. Note that
    /// the matcher worker will only become aware of the new pattern after a
    /// call to [`tick`](Nucleo::tick).
    pub pattern: MultiPattern,
}

impl<T: Sync + Send + 'static> Nucleo<T> {
    /// Constructs a new `nucleo` worker threadpool with the provided `config`.
    ///
    /// `notify` is called whenever new information is available and
    /// [`tick`](Nucleo::tick) should be called. Note that `notify` is not
    /// debounced; that should be handled by the downstream crate (for example,
    /// debouncing to only redraw at most every 1/60 seconds).
    ///
    /// If `None` is passed for the number of worker threads, nucleo will use
    /// one thread per hardware thread.
    ///
    /// Nucleo can match items with multiple orthogonal properties. `columns`
    /// indicates how many matching columns each item (and the pattern) has. The
    /// number of columns cannot be changed after construction.
    pub fn new(
        config: Config,
        notify: Arc<dyn Fn() + Sync + Send>,
        num_threads: Option<usize>,
        columns: u32,
    ) -> Self {
        Self::with_match_list_config(
            config,
            notify,
            num_threads,
            columns,
            MatchListConfig::DEFAULT,
        )
    }

    /// Constructs a new worker threadpool with the provided configuration, with pre-defined
    /// configuration for the match list.
    pub fn with_match_list_config(
        config: Config,
        notify: Arc<dyn Fn() + Sync + Send>,
        num_threads: Option<usize>,
        columns: u32,
        match_list_config: MatchListConfig,
    ) -> Self {
        let (pool, worker) = Worker::new(
            num_threads,
            config,
            notify.clone(),
            columns,
            match_list_config,
        );
        Self {
            canceled: worker.canceled.clone(),
            should_notify: worker.should_notify.clone(),
            items: worker.items.clone(),
            pool,
            pattern: MultiPattern::new(columns as usize),
            snapshot: Snapshot {
                matches: Vec::with_capacity(2 * 1024),
                pattern: MultiPattern::new(columns as usize),
                item_count: 0,
                items: worker.items.clone(),
            },
            worker: Arc::new(Mutex::new(worker)),
            state: State::Init,
            notify,
        }
    }

    /// Returns the total number of active injectors.
    pub fn active_injectors(&self) -> usize {
        Arc::strong_count(&self.items)
            - self.state.matcher_item_refs()
            - (Arc::ptr_eq(&self.snapshot.items, &self.items)) as usize
    }

    /// Returns a snapshot of the current matcher state.
    ///
    /// This method is very cheap and can be called every time a snapshot is required. The
    /// snapshot will not change unless [`tick`](Nucleo::tick) is called.
    pub fn snapshot(&self) -> &Snapshot<T> {
        &self.snapshot
    }

    /// Returns an injector that can be used for adding candidates to the matcher.
    pub fn injector(&self) -> Injector<T> {
        Injector {
            items: self.items.clone(),
            notify: self.notify.clone(),
        }
    }

    /// Restart the the item stream. Removes all items and disconnects all
    /// previously created injectors from this instance. If `clear_snapshot`
    /// is `true` then all items and matches are removed from the [`Snapshot`]
    /// immediately. Otherwise the snapshot will keep the current matches until
    /// the matcher has run again.
    ///
    /// # Note
    ///
    /// The injectors will continue to function but they will not affect this
    /// instance anymore. The old items will only be dropped when all injectors
    /// and detached items are dropped.
    pub fn restart(&mut self, clear_snapshot: bool) {
        self.canceled.store(true, Ordering::Relaxed);
        self.items = Arc::new(boxcar::Vec::with_capacity(1024, self.items.columns()));
        self.state = State::Cleared;
        if clear_snapshot {
            self.snapshot.clear(self.items.clone());
        }
    }

    /// Update the internal configuration.
    pub fn update_config(&mut self, config: Config) {
        self.worker.lock().update_config(config);
    }

    /// Set whether the matcher should sort search results by score after
    /// matching. Defaults to true.
    pub fn sort_results(&mut self, sort_results: bool) {
        self.worker.lock().sort_results(sort_results);
    }

    /// Set whether the matcher should reverse the order of the input.
    /// Defaults to false.
    pub fn reverse_items(&mut self, reverse_items: bool) {
        self.worker.lock().reverse_items(reverse_items);
    }

    /// Update the internal state to reflect any changes from the background worker
    /// threads.
    ///
    /// This is the main way to interact with the matcher, and should be called
    /// regularly (for example each time a frame is rendered). To avoid excessive
    /// redraws this method will wait `timeout` milliseconds for the
    /// worker threads to finish. It is recommend to set the timeout to 10ms.
    pub fn tick(&mut self, timeout: u64) -> Status {
        self.should_notify.store(false, atomic::Ordering::Relaxed);
        let status = self.pattern.status();
        let canceled = status != pattern::Status::Unchanged || self.state.canceled();
        let mut res = self.tick_inner(timeout, canceled, status);
        if !canceled {
            return res;
        }
        self.state = State::Fresh;
        let status2 = self.tick_inner(timeout, false, pattern::Status::Unchanged);
        res.changed |= status2.changed;
        res.running = status2.running;
        res
    }

    fn tick_inner(&mut self, timeout: u64, canceled: bool, status: pattern::Status) -> Status {
        let mut inner = if canceled {
            self.pattern.reset_status();
            self.canceled.store(true, atomic::Ordering::Relaxed);
            self.worker.lock_arc()
        } else {
            let Some(worker) = self.worker.try_lock_arc_for(Duration::from_millis(timeout)) else {
                self.should_notify.store(true, Ordering::Release);
                return Status {
                    changed: false,
                    running: true,
                };
            };
            worker
        };

        let pending_update = inner.needs_rescore || inner.needs_sort;
        let changed = inner.running && !pending_update;

        let running = canceled || pending_update || self.items.count() > inner.item_count();
        if inner.running {
            inner.running = false;
            if !inner.was_canceled && !self.state.canceled() && !pending_update {
                self.snapshot.update(&inner);
            }
        }
        if running {
            let status = if inner.needs_rescore {
                pattern::Status::Rescore
            } else {
                status
            };
            inner.needs_rescore = false;
            inner.needs_sort = false;
            inner.pattern.clone_from(&self.pattern);
            self.canceled.store(false, atomic::Ordering::Relaxed);
            if !canceled {
                self.should_notify.store(true, atomic::Ordering::Release);
            }
            let cleared = self.state.cleared();
            if cleared {
                inner.items = self.items.clone();
            }
            self.pool
                .spawn(move || unsafe { inner.run(status, cleared) });
        }
        Status { changed, running }
    }
}

impl<T> Drop for Nucleo<T> {
    fn drop(&mut self) {
        // we ensure the worker quits before dropping items to ensure that
        // the worker can always assume the items outlive it
        self.canceled.store(true, atomic::Ordering::Relaxed);
        let lock = self.worker.try_lock_for(Duration::from_secs(1));
        if lock.is_none() {
            unreachable!("thread pool failed to shutdown properly")
        }
    }
}
