use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait Observer<T>: Send + Sync {
    async fn notify(&self, event: &T);
}

pub struct Observable<T> {
    observers: Vec<Arc<dyn Observer<T>>>,
}

impl<T> Observable<T> {
    pub fn new() -> Self {
        Self {
            observers: Vec::new(),
        }
    }

    pub fn subscribe(&mut self, observer: Arc<dyn Observer<T>>) {
        self.observers.push(observer);
    }

    pub async fn notify_all(&self, event: &T) {
        for observer in &self.observers {
            observer.notify(event).await;
        }
    }
}

impl<T> Default for Observable<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex;

    struct TestObserver {
        received: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Observer<String> for TestObserver {
        async fn notify(&self, event: &String) {
            self.received.lock().await.push(event.clone());
        }
    }

    #[tokio::test]
    async fn test_observer_notifies_all() {
        let mut observable = Observable::new();

        let received = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();

        observable.subscribe(Arc::new(TestObserver {
            received: received_clone,
        }));

        observable.notify_all(&"event1".to_string()).await;
        observable.notify_all(&"event2".to_string()).await;

        let events = received.lock().await;
        assert_eq!(*events, vec!["event1", "event2"]);
    }

    #[tokio::test]
    async fn test_multiple_observers() {
        let mut observable = Observable::new();

        let received1 = Arc::new(Mutex::new(Vec::new()));
        let received2 = Arc::new(Mutex::new(Vec::new()));

        observable.subscribe(Arc::new(TestObserver {
            received: received1.clone(),
        }));
        observable.subscribe(Arc::new(TestObserver {
            received: received2.clone(),
        }));

        observable.notify_all(&"event".to_string()).await;

        assert_eq!(*received1.lock().await, vec!["event"]);
        assert_eq!(*received2.lock().await, vec!["event"]);
    }
}
