//! Research for one topic: search, pick, read, and widen the search until the
//! topic has what it needs. [`TopicResearch`] is the algorithm, with the steps
//! that differ by caller left to it; [`Rounds`] is the order of widening.
use super::web_research::{self, Candidate, NeedSearch, PlannedNeed};
use lattice_core::application::contracts::web::WebSearchResult;
use lattice_core::domain::sourcing::NeedKind;
use lattice_core::shared::error::Result;

/// The general kinds a topic is widened to, in the order they are asked.
const GENERAL_KINDS: [NeedKind; 4] = [
    NeedKind::Web,
    NeedKind::Encyclopedia,
    NeedKind::Community,
    NeedKind::Paper,
];

#[derive(Clone, Copy)]
enum Stage {
    Planned,
    Deeper,
    OtherKinds,
    Shorter,
    Done,
}

/// The searches one round asks, each once.
pub(in crate::features::learning) struct Round<'r> {
    searches: Vec<&'r mut NeedSearch<'static>>,
}

impl<'r> Round<'r> {
    pub(in crate::features::learning) fn len(&self) -> usize {
        self.searches.len()
    }

    pub(in crate::features::learning) fn iter_mut(
        &mut self,
    ) -> std::slice::IterMut<'_, &'r mut NeedSearch<'static>> {
        self.searches.iter_mut()
    }

    /// The (query, kind, page) of each search, in order.
    pub(in crate::features::learning) fn asks(&self) -> Vec<(String, &'static str, usize)> {
        self.searches.iter().map(|search| ask(search)).collect()
    }
}

fn ask(search: &NeedSearch<'_>) -> (String, &'static str, usize) {
    let need = search.need();
    (need.query.clone(), need.kind.as_str(), search.page())
}

/// How a topic's search widens, round by round: its planned searches, the next
/// page of those that had more, the general kinds not yet asked with the first
/// planned query, then the title on the web. A round with nothing left to ask
/// is skipped, and no search is asked twice.
pub(in crate::features::learning) struct Rounds {
    title: String,
    first_query: Option<String>,
    searches: Vec<NeedSearch<'static>>,
    planned: usize,
    asked: Vec<(String, &'static str, usize)>,
    stage: Stage,
}

impl Rounds {
    /// `needs` are the topic's planned needs; the planned round also takes the
    /// web and encyclopedia searches [`web_research::with_breadth`] adds.
    pub(in crate::features::learning) fn new(needs: &[PlannedNeed], title: &str) -> Self {
        let searches: Vec<NeedSearch<'static>> = web_research::with_breadth(needs, title)
            .into_iter()
            .map(NeedSearch::owned)
            .collect();
        Self {
            title: title.trim().to_owned(),
            first_query: needs.first().map(|need| need.query.clone()),
            planned: searches.len(),
            stage: if searches.is_empty() {
                Stage::Done
            } else {
                Stage::Planned
            },
            searches,
            asked: Vec::new(),
        }
    }

    /// The next round that has something to ask; `None` once there is none.
    pub(in crate::features::learning) fn next(&mut self) -> Option<Round<'_>> {
        let chosen = loop {
            let stage = self.stage;
            let chosen: Vec<usize> = match stage {
                Stage::Planned => {
                    self.stage = Stage::Deeper;
                    (0..self.planned).collect()
                }
                Stage::Deeper => {
                    self.stage = Stage::OtherKinds;
                    self.searches
                        .iter()
                        .take(self.planned)
                        .enumerate()
                        .filter(|(_, search)| search.page() > 1 && !search.is_done())
                        .map(|(i, _)| i)
                        .collect()
                }
                Stage::OtherKinds => {
                    self.stage = Stage::Shorter;
                    let kinds: Vec<NeedKind> = GENERAL_KINDS
                        .into_iter()
                        .filter(|kind| {
                            !self
                                .asked
                                .iter()
                                .any(|(_, asked, _)| *asked == kind.as_str())
                        })
                        .collect();
                    match self.first_query.clone() {
                        Some(query) => kinds
                            .into_iter()
                            .filter_map(|kind| {
                                self.add(PlannedNeed {
                                    kind,
                                    ..PlannedNeed::web(query.clone())
                                })
                            })
                            .collect(),
                        None => Vec::new(),
                    }
                }
                Stage::Shorter => {
                    self.stage = Stage::Done;
                    if self.title.is_empty() {
                        Vec::new()
                    } else {
                        self.add(PlannedNeed::web(self.title.clone()))
                            .into_iter()
                            .collect()
                    }
                }
                Stage::Done => return None,
            };
            if !chosen.is_empty() {
                break chosen;
            }
        };
        for (i, search) in self.searches.iter().enumerate() {
            if chosen.contains(&i) {
                self.asked.push(ask(search));
            }
        }
        Some(Round {
            searches: self
                .searches
                .iter_mut()
                .enumerate()
                .filter(|(i, _)| chosen.contains(i))
                .map(|(_, search)| search)
                .collect(),
        })
    }

    /// Keeps a first-page search of `need` unless the same one was asked,
    /// returning where it is kept.
    fn add(&mut self, need: PlannedNeed) -> Option<usize> {
        let again = self.asked.iter().any(|(query, kind, page)| {
            *query == need.query && *kind == need.kind.as_str() && *page == 1
        });
        if again {
            return None;
        }
        self.searches.push(NeedSearch::owned(need));
        Some(self.searches.len() - 1)
    }
}

/// Research for one topic, widened round by round until `goal` pages are
/// saved. What a search returns, how a pick is made and how a pick is read
/// and saved are the caller's.
#[async_trait::async_trait]
pub(in crate::features::learning) trait TopicResearch: Send {
    /// Runs the round's searches and returns the candidates this topic has not
    /// been shown before.
    async fn search(&mut self, round: &mut Round<'_>) -> Result<Vec<Candidate>>;

    /// The candidates worth reading.
    async fn pick(&mut self, found: Vec<Candidate>) -> Result<Vec<WebSearchResult>>;

    /// Reads the picked pages and saves what reads, returning how many were
    /// saved.
    async fn read(&mut self, picked: Vec<WebSearchResult>) -> Result<usize>;

    /// Returns how many pages were saved, which is at least `goal` unless the
    /// rounds ran out first.
    async fn research(&mut self, mut rounds: Rounds, goal: usize) -> Result<usize> {
        let mut saved = 0;
        while saved < goal {
            let Some(mut round) = rounds.next() else {
                break;
            };
            let found = self.search(&mut round).await?;
            if found.is_empty() {
                continue;
            }
            let picked = self.pick(found).await?;
            if picked.is_empty() {
                continue;
            }
            saved += self.read(picked).await?;
        }
        Ok(saved)
    }
}
