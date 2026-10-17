//! Loaded model instances keyed by what they are, any number at once.
//!
//! Each key loads at most once at a time; different keys load side by side.
//! An instance is handed out until it dies, is invalidated, or is evicted to
//! make room, and whoever already holds one keeps it until they drop it.
//! Holders that pin an instance for a piece of work take a [`Lease`], which is
//! what makes an instance busy: only idle instances are ever evicted.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::hash::Hash;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use chrono::{DateTime, Utc};

/// Whether a loaded value can still serve. A local model server can die under
/// its client (jetsam, a Metal fault, a GPU reset after wake); handing that
/// client out again would fail every request, so a dead value is a miss.
pub(crate) trait Alive {
    fn is_alive(&self) -> bool;
}

pub(crate) struct InstancePool<K, V, H> {
    state: Mutex<State<K, V, H>>,
    /// Bumped by every invalidation. A load publishes only when none happened
    /// since its caller read the configuration it loads from.
    epoch: AtomicU64,
    next_instance_id: AtomicU64,
    /// Orders uses, so least-recently-used holds even when two uses share a
    /// clock reading.
    use_clock: Arc<AtomicU64>,
}

struct State<K, V, H> {
    live: HashMap<K, Slot<V, H>>,
    /// Invalidated instances still pinned by running work: no longer handed
    /// out, still loaded, still worth naming.
    retired: Vec<(K, Weak<V>, Arc<Usage<H>>)>,
    /// One load at a time per key.
    gates: HashMap<K, Weak<tokio::sync::Mutex<()>>>,
}

struct Slot<V, H> {
    value: Arc<V>,
    usage: Arc<Usage<H>>,
}

/// How one instance is being used.
pub(crate) struct Usage<H> {
    instance_id: u64,
    loaded_at: DateTime<Utc>,
    last_used: Mutex<(u64, DateTime<Utc>)>,
    in_flight: AtomicU32,
    holders: Mutex<Vec<(u64, H)>>,
    next_lease: AtomicU64,
}

/// One holder's pin on an instance. Dropping it releases the pin.
pub(crate) struct Lease<H> {
    usage: Arc<Usage<H>>,
    id: u64,
    clock: Arc<AtomicU64>,
}

/// An instance handed out by [`InstancePool::get_or_load`].
pub(crate) struct Loaded<V, H> {
    pub(crate) value: Arc<V>,
    pub(crate) instance_id: u64,
    /// Whether it was already loaded.
    pub(crate) reused: bool,
    /// The caller's pin, when it asked for one.
    pub(crate) lease: Option<Lease<H>>,
}

/// One instance's use, at the moment it was read.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EntryStats<H> {
    pub(crate) instance_id: u64,
    pub(crate) loaded_at: DateTime<Utc>,
    pub(crate) last_used_at: DateTime<Utc>,
    pub(crate) in_flight: u32,
    pub(crate) holders: Vec<H>,
    /// Invalidated, but still pinned by work that started before.
    pub(crate) retired: bool,
}

/// An instance and its use, from [`InstancePool::snapshot`].
pub(crate) struct Entry<K, V, H> {
    pub(crate) key: K,
    pub(crate) value: Arc<V>,
    pub(crate) stats: EntryStats<H>,
}

impl<K, V, H> InstancePool<K, V, H>
where
    K: Clone + Eq + Hash,
    V: Alive,
    H: Clone,
{
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(State {
                live: HashMap::new(),
                retired: Vec::new(),
                gates: HashMap::new(),
            }),
            epoch: AtomicU64::new(0),
            next_instance_id: AtomicU64::new(1),
            use_clock: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Read before the configuration a load is built from, and hand to
    /// [`Self::get_or_load`].
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// The live instance for `key`, without loading or pinning it.
    pub(crate) fn get(&self, key: &K) -> Option<Arc<V>> {
        self.lock()
            .live
            .get(key)
            .filter(|slot| slot.value.is_alive())
            .map(|slot| Arc::clone(&slot.value))
    }

    /// The instance for `key`, loading it with `load` when there is none.
    ///
    /// Concurrent callers for one key share one load; callers for different
    /// keys load in parallel. A dead instance is dropped and loaded again.
    /// Errors are not cached. A load that an invalidation overtook — any since
    /// `epoch` was read — still answers its caller but is never handed out
    /// again; while its holder pins it, it is listed as retired.
    /// `holder`, when given, pins the instance before anything can evict it.
    pub(crate) async fn get_or_load<E, F, Fut>(
        &self,
        key: K,
        epoch: u64,
        holder: Option<H>,
        load: F,
    ) -> Result<Loaded<V, H>, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V, E>>,
    {
        let gate = {
            let mut state = self.lock();
            if let Some(hit) = self.hit(&mut state, &key, &holder) {
                return Ok(hit);
            }
            state.gates.retain(|_, gate| gate.strong_count() > 0);
            match state.gates.get(&key).and_then(Weak::upgrade) {
                Some(gate) => gate,
                None => {
                    let gate = Arc::new(tokio::sync::Mutex::new(()));
                    state.gates.insert(key.clone(), Arc::downgrade(&gate));
                    gate
                }
            }
        };
        let _turn = gate.lock().await;
        if let Some(hit) = self.hit(&mut self.lock(), &key, &holder) {
            return Ok(hit);
        }

        let value = Arc::new(load().await?);
        let usage = Arc::new(Usage::new(
            self.next_instance_id.fetch_add(1, Ordering::Relaxed),
            self.tick(),
        ));
        let mut state = self.lock();
        let lease = holder.map(|holder| self.hold(&usage, holder));
        if self.epoch() == epoch {
            state.live.insert(
                key,
                Slot {
                    value: Arc::clone(&value),
                    usage: Arc::clone(&usage),
                },
            );
        } else if lease.is_some() {
            // It may have been built from settings the invalidation replaced,
            // so it is not handed out; but it is answering and holds memory,
            // so whoever asks who holds the memory must still find it.
            state
                .retired
                .push((key, Arc::downgrade(&value), Arc::clone(&usage)));
        }
        Ok(Loaded {
            value,
            instance_id: usage.instance_id,
            reused: false,
            lease,
        })
    }

    /// Stop handing out every instance whose key matches `matches`, and return
    /// those keys. Holders keep what they hold; pinned instances stay listed
    /// as retired until their last pin is released.
    pub(crate) fn invalidate(&self, matches: impl Fn(&K) -> bool) -> Vec<K> {
        let mut state = self.lock();
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let keys: Vec<K> = state.live.keys().filter(|k| matches(k)).cloned().collect();
        for key in &keys {
            if let Some(slot) = state.live.remove(key) {
                if slot.usage.in_flight() > 0 {
                    state
                        .retired
                        .push((key.clone(), Arc::downgrade(&slot.value), slot.usage));
                }
            }
        }
        keys
    }

    /// Unload idle servers, least recently used first, until at least
    /// `needed` bytes are freed. Instances whose keys name one `server` are
    /// clients of one process: it is freed only when the last of them goes,
    /// so they are priced once, unloaded together, and only when none is in
    /// use. `freed` says what unloading an instance's server frees, or `None`
    /// when it frees nothing worth having. When every idle server together
    /// frees less than `needed`, nothing is unloaded: an eviction that cannot
    /// make room only costs the next request a reload. The bytes a server
    /// frees are reported against the first of its instances, and zero
    /// against the rest.
    pub(crate) fn evict_idle<S: Eq + Hash>(
        &self,
        server: impl Fn(&K) -> S,
        freed: impl Fn(&K, &V) -> Option<u64>,
        needed: u64,
    ) -> Vec<(K, u64, EntryStats<H>)> {
        let mut state = self.lock();
        let busy: HashSet<S> = state
            .live
            .iter()
            .filter(|(_, slot)| slot.usage.in_flight() > 0)
            .map(|(key, _)| server(key))
            .chain(
                state
                    .retired
                    .iter()
                    .filter(|(_, value, usage)| usage.in_flight() > 0 && value.strong_count() > 0)
                    .map(|(key, ..)| server(key)),
            )
            .collect();
        // Per idle server: what unloading it frees, its latest use, and its
        // instances.
        let mut servers: HashMap<S, (u64, u64, Vec<K>)> = HashMap::new();
        for (key, slot) in &state.live {
            let id = server(key);
            if busy.contains(&id) {
                continue;
            }
            let entry = servers.entry(id).or_insert((0, 0, Vec::new()));
            entry.0 = entry.0.max(freed(key, &slot.value).unwrap_or(0));
            entry.1 = entry.1.max(slot.usage.last_used().0);
            entry.2.push(key.clone());
        }
        let mut idle: Vec<(u64, u64, Vec<K>)> = servers
            .into_values()
            .filter(|(bytes, ..)| *bytes > 0)
            .collect();
        if idle
            .iter()
            .map(|(bytes, ..)| *bytes)
            .fold(0, u64::saturating_add)
            < needed
        {
            return Vec::new();
        }
        idle.sort_by_key(|(_, tick, _)| *tick);
        let mut evicted = Vec::new();
        let mut total = 0u64;
        for (bytes, _, keys) in idle {
            if total >= needed {
                break;
            }
            total = total.saturating_add(bytes);
            for (index, key) in keys.into_iter().enumerate() {
                if let Some(slot) = state.live.remove(&key) {
                    let reported = if index == 0 { bytes } else { 0 };
                    evicted.push((key, reported, slot.usage.stats(false)));
                }
            }
        }
        evicted
    }

    /// Every instance loaded now: those handed out, then retired ones still
    /// pinned.
    pub(crate) fn snapshot(&self) -> Vec<Entry<K, V, H>> {
        let mut state = self.lock();
        state
            .retired
            .retain(|(_, value, usage)| usage.in_flight() > 0 && value.strong_count() > 0);
        let live = state.live.iter().map(|(key, slot)| Entry {
            key: key.clone(),
            value: Arc::clone(&slot.value),
            stats: slot.usage.stats(false),
        });
        let retired = state.retired.iter().filter_map(|(key, value, usage)| {
            Some(Entry {
                key: key.clone(),
                value: value.upgrade()?,
                stats: usage.stats(true),
            })
        });
        live.chain(retired).collect()
    }

    fn lock(&self) -> MutexGuard<'_, State<K, V, H>> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn tick(&self) -> u64 {
        self.use_clock.fetch_add(1, Ordering::Relaxed)
    }

    /// The live instance for `key`, pinned for `holder`. A dead one is dropped
    /// so the caller loads it again.
    fn hit(&self, state: &mut State<K, V, H>, key: &K, holder: &Option<H>) -> Option<Loaded<V, H>> {
        let slot = state.live.get(key)?;
        if !slot.value.is_alive() {
            tracing::warn!(
                instance_id = slot.usage.instance_id,
                "Loaded model's server is no longer running; starting it again"
            );
            state.live.remove(key);
            return None;
        }
        slot.usage.touch(self.tick());
        Some(Loaded {
            value: Arc::clone(&slot.value),
            instance_id: slot.usage.instance_id,
            reused: true,
            lease: holder.clone().map(|holder| self.hold(&slot.usage, holder)),
        })
    }

    fn hold(&self, usage: &Arc<Usage<H>>, holder: H) -> Lease<H> {
        let id = usage.next_lease.fetch_add(1, Ordering::Relaxed);
        usage.in_flight.fetch_add(1, Ordering::SeqCst);
        usage.lock_holders().push((id, holder));
        usage.touch(self.tick());
        Lease {
            usage: Arc::clone(usage),
            id,
            clock: Arc::clone(&self.use_clock),
        }
    }
}

impl<H: Clone> Usage<H> {
    fn new(instance_id: u64, tick: u64) -> Self {
        let now = Utc::now();
        Self {
            instance_id,
            loaded_at: now,
            last_used: Mutex::new((tick, now)),
            in_flight: AtomicU32::new(0),
            holders: Mutex::new(Vec::new()),
            next_lease: AtomicU64::new(0),
        }
    }

    fn in_flight(&self) -> u32 {
        self.in_flight.load(Ordering::SeqCst)
    }

    fn last_used(&self) -> (u64, DateTime<Utc>) {
        *self
            .last_used
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn touch(&self, tick: u64) {
        *self
            .last_used
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = (tick, Utc::now());
    }

    fn lock_holders(&self) -> MutexGuard<'_, Vec<(u64, H)>> {
        self.holders
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn stats(&self, retired: bool) -> EntryStats<H> {
        EntryStats {
            instance_id: self.instance_id,
            loaded_at: self.loaded_at,
            last_used_at: self.last_used().1,
            in_flight: self.in_flight(),
            holders: self
                .lock_holders()
                .iter()
                .map(|(_, holder)| holder.clone())
                .collect(),
            retired,
        }
    }
}

impl<H> Drop for Lease<H> {
    fn drop(&mut self) {
        self.usage
            .holders
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|(id, _)| *id != self.id);
        self.usage.in_flight.fetch_sub(1, Ordering::SeqCst);
        *self
            .usage
            .last_used
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            (self.clock.fetch_add(1, Ordering::Relaxed), Utc::now());
    }
}

#[cfg(test)]
mod tests;
