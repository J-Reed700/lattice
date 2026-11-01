//! What builds and searches see of the open folder, readable without waiting
//! for an open or a close in progress.

use crate::features::explorer::index::dto::FolderIndexStatusDto;
use crate::features::explorer::index::run::StatusCell;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The open folder as builds and searches see it.
#[derive(Clone)]
pub struct OpenView {
    pub root: PathBuf,
    pub status: Arc<StatusCell>,
}

#[derive(Default)]
struct State {
    open: Option<OpenView>,
    /// Each folder's latest word from a build, kept past its build's end: a
    /// folder shown while its build finishes takes it from here.
    reported: HashMap<PathBuf, FolderIndexStatusDto>,
}

/// One lock over both, so a build's report either reaches the folder already
/// shown or is there for [`Self::catch_up`].
#[derive(Default)]
pub struct FolderView {
    state: Mutex<State>,
}

impl FolderView {
    pub fn open(&self) -> Option<OpenView> {
        self.state.lock().open.clone()
    }

    pub fn show(&self, view: OpenView) {
        self.state.lock().open = Some(view);
    }

    pub fn hide(&self, root: &Path) {
        let mut state = self.state.lock();
        if state.open.as_ref().is_some_and(|open| open.root == root) {
            state.open = None;
        }
    }

    /// A build's status, shown when its folder is open.
    pub fn report(&self, root: &Path, status: &FolderIndexStatusDto) {
        let mut state = self.state.lock();
        state.reported.insert(root.to_path_buf(), status.clone());
        if let Some(open) = state.open.as_ref().filter(|open| open.root == root) {
            open.status.replace(status.clone());
        }
    }

    /// Brings in what `root`'s running build said before the folder was shown.
    pub fn catch_up(&self, root: &Path) {
        let state = self.state.lock();
        let open = state.open.as_ref().filter(|open| open.root == root);
        if let (Some(open), Some(reported)) = (open, state.reported.get(root)) {
            open.status.replace(reported.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::explorer::index::dto::FolderIndexState;

    fn status(state: FolderIndexState) -> FolderIndexStatusDto {
        FolderIndexStatusDto::new("root", "root", state)
    }

    fn shown(view: &FolderView, root: &Path) -> Arc<StatusCell> {
        let cell = Arc::new(StatusCell::new(status(FolderIndexState::Scanning)));
        view.show(OpenView {
            root: root.to_path_buf(),
            status: Arc::clone(&cell),
        });
        cell
    }

    #[test]
    fn a_folder_shown_as_its_build_ends_still_gets_the_last_word() {
        let view = FolderView::default();
        let root = Path::new("project");
        view.report(root, &status(FolderIndexState::Ready));
        let cell = shown(&view, root);
        assert_eq!(cell.get().state, FolderIndexState::Scanning);
        view.catch_up(root);
        assert_eq!(cell.get().state, FolderIndexState::Ready);
    }

    #[test]
    fn a_build_reports_to_its_open_folder_and_no_other() {
        let view = FolderView::default();
        let cell = shown(&view, Path::new("project"));
        view.report(Path::new("other"), &status(FolderIndexState::Ready));
        assert_eq!(cell.get().state, FolderIndexState::Scanning);
        view.report(Path::new("project"), &status(FolderIndexState::Indexing));
        assert_eq!(cell.get().state, FolderIndexState::Indexing);
    }

    #[test]
    fn catching_up_on_another_folders_report_changes_nothing() {
        let view = FolderView::default();
        let cell = shown(&view, Path::new("project"));
        view.report(Path::new("other"), &status(FolderIndexState::Ready));
        view.catch_up(Path::new("other"));
        view.catch_up(Path::new("project"));
        assert_eq!(cell.get().state, FolderIndexState::Scanning);
    }

    #[test]
    fn only_the_folder_shown_can_be_hidden() {
        let view = FolderView::default();
        shown(&view, Path::new("project"));
        view.hide(Path::new("other"));
        assert!(view.open().is_some());
        view.hide(Path::new("project"));
        assert!(view.open().is_none());
    }
}
