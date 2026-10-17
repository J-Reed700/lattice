use super::*;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// A loaded value whose server the test can kill.
struct Model {
    name: &'static str,
    alive: Arc<AtomicBool>,
}

impl Model {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            alive: Arc::new(AtomicBool::new(true)),
        }
    }
}

impl Alive for Model {
    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }
}

type Pool = InstancePool<&'static str, Model, &'static str>;

async fn load(
    pool: &Pool,
    key: &'static str,
    holder: Option<&'static str>,
) -> Loaded<Model, &'static str> {
    pool.get_or_load(key, pool.epoch(), holder, || async {
        Ok::<_, ()>(Model::new(key))
    })
    .await
    .unwrap()
}

fn holders_of(pool: &Pool, key: &str) -> Vec<&'static str> {
    pool.snapshot()
        .into_iter()
        .find(|entry| entry.key == key && !entry.stats.retired)
        .map(|entry| entry.stats.holders)
        .unwrap_or_default()
}

#[tokio::test]
async fn concurrent_requests_for_one_key_share_one_load() {
    let pool = Arc::new(Pool::new());
    let loads = Arc::new(AtomicU32::new(0));
    let request = |pool: Arc<Pool>, loads: Arc<AtomicU32>| async move {
        pool.get_or_load("chat", pool.epoch(), None, || async {
            loads.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok::<_, ()>(Model::new("chat"))
        })
        .await
        .unwrap()
    };

    let (first, second) = tokio::join!(
        request(Arc::clone(&pool), Arc::clone(&loads)),
        request(Arc::clone(&pool), Arc::clone(&loads))
    );

    assert_eq!(loads.load(Ordering::SeqCst), 1);
    assert_eq!(first.instance_id, second.instance_id);
    assert!(Arc::ptr_eq(&first.value, &second.value));
    assert_ne!(first.reused, second.reused, "one loaded, the other reused");
}

#[tokio::test]
async fn different_keys_load_side_by_side() {
    let pool = Pool::new();
    // Each load waits for the other to start: loads serialised behind one
    // lock would never both arrive.
    let both_started = Arc::new(tokio::sync::Barrier::new(2));
    let request = |key: &'static str| {
        let both_started = Arc::clone(&both_started);
        pool.get_or_load(key, pool.epoch(), None, move || async move {
            both_started.wait().await;
            Ok::<_, ()>(Model::new(key))
        })
    };

    let (chat, utility) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(request("chat"), request("utility"))
    })
    .await
    .expect("two keys must not wait on each other");

    assert_ne!(chat.unwrap().instance_id, utility.unwrap().instance_id);
}

#[tokio::test]
async fn a_dead_instance_is_dropped_and_loaded_again() {
    let pool = Pool::new();
    let first = load(&pool, "chat", None).await;
    first.value.alive.store(false, Ordering::SeqCst);

    let second = load(&pool, "chat", None).await;

    assert!(!second.reused);
    assert_ne!(first.instance_id, second.instance_id);
    assert!(second.value.is_alive());
}

#[tokio::test]
async fn failed_loads_are_not_remembered() {
    let pool = Pool::new();
    let failed = pool
        .get_or_load("chat", pool.epoch(), None, || async {
            Err::<Model, _>("refused")
        })
        .await;
    assert!(matches!(failed, Err("refused")));

    assert!(!load(&pool, "chat", None).await.reused);
}

#[tokio::test]
async fn invalidation_stops_handing_out_an_instance_but_its_holders_keep_it() {
    let pool = Pool::new();
    let held = load(&pool, "chat", Some("answering")).await;
    load(&pool, "utility", None).await;

    let matched = pool.invalidate(|key| *key == "chat");

    assert_eq!(matched, vec!["chat"]);
    assert_eq!(held.value.name, "chat", "the holder's instance is intact");
    let replacement = load(&pool, "chat", None).await;
    assert!(!replacement.reused);
    assert_ne!(replacement.instance_id, held.instance_id);
    assert!(load(&pool, "utility", None).await.reused, "other keys stay");

    let retired: Vec<_> = pool
        .snapshot()
        .into_iter()
        .filter(|entry| entry.stats.retired)
        .map(|entry| (entry.stats.instance_id, entry.stats.holders))
        .collect();
    assert_eq!(retired, vec![(held.instance_id, vec!["answering"])]);

    drop(held);
    assert!(
        pool.snapshot().iter().all(|entry| !entry.stats.retired),
        "a retired instance leaves the list with its last pin"
    );
}

#[tokio::test]
async fn a_load_overtaken_by_invalidation_answers_its_caller_but_is_not_kept() {
    let pool = Pool::new();
    let epoch = pool.epoch();
    pool.invalidate(|_| false);

    let stale = pool
        .get_or_load("chat", epoch, None, || async {
            Ok::<_, ()>(Model::new("chat"))
        })
        .await
        .unwrap();

    assert_eq!(stale.value.name, "chat");
    assert!(pool.get(&"chat").is_none());
}

#[tokio::test]
async fn leases_count_what_is_in_flight_and_release_on_drop() {
    let pool = Pool::new();
    let turn = load(&pool, "chat", Some("answering in 'Notes'")).await;
    let job = load(&pool, "chat", Some("preparing a lesson")).await;
    assert_eq!(
        holders_of(&pool, "chat"),
        vec!["answering in 'Notes'", "preparing a lesson"]
    );

    drop(turn);
    assert_eq!(holders_of(&pool, "chat"), vec!["preparing a lesson"]);
    drop(job);
    let stats = pool.snapshot().remove(0).stats;
    assert_eq!((stats.in_flight, stats.holders), (0, vec![]));
}

#[tokio::test]
async fn eviction_skips_pinned_instances() {
    let pool = Pool::new();
    let _busy = load(&pool, "chat", Some("answering")).await;
    load(&pool, "utility", None).await;

    let evicted = pool.evict_idle(|_, _| Some(10), 10);

    assert_eq!(
        evicted
            .iter()
            .map(|(key, bytes, _)| (*key, *bytes))
            .collect::<Vec<_>>(),
        vec![("utility", 10)]
    );
    assert!(pool.get(&"chat").is_some());
    assert!(pool.get(&"utility").is_none());
}

#[tokio::test]
async fn eviction_unloads_nothing_when_the_idle_instances_cannot_make_room() {
    let pool = Pool::new();
    let _busy = load(&pool, "chat", Some("answering")).await;
    load(&pool, "utility", None).await;

    assert!(pool.evict_idle(|_, _| Some(10), 11).is_empty());
    assert!(pool.get(&"utility").is_some());
}

#[tokio::test]
async fn eviction_takes_the_least_recently_used_first_and_stops_once_there_is_room() {
    let pool = Pool::new();
    for key in ["a", "b", "c"] {
        load(&pool, key, None).await;
    }
    load(&pool, "a", None).await;
    let only_c_frees_nothing = |key: &&str, _: &Model| (*key != "c").then_some(10);

    let evicted = pool.evict_idle(only_c_frees_nothing, 10);

    assert_eq!(
        evicted.iter().map(|(key, ..)| *key).collect::<Vec<_>>(),
        vec!["b"],
        "b is the least recently used of those that free memory"
    );
    assert!(pool.get(&"a").is_some() && pool.get(&"c").is_some());
}
