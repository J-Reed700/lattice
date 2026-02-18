use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::channel;

pub struct FileWatcher {
    watcher: RecommendedWatcher,
}

impl FileWatcher {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (tx, rx) = channel();

        let watcher =
            notify::recommended_watcher(move |res: Result<Event, notify::Error>| match res {
                Ok(event) => {
                    if let Err(e) = tx.send(event) {
                        eprintln!("Error sending event: {}", e);
                    }
                }
                Err(e) => eprintln!("Watch error: {:?}", e),
            })?;

        Ok(FileWatcher { watcher })
    }

    pub fn watch(&mut self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        self.watcher.watch(path, RecursiveMode::Recursive)?;
        Ok(())
    }
}
