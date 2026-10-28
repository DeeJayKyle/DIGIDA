//! Finding a headphone among AutoEQ's measurements and playing its correction.

use std::sync::Arc;

use gpui::{Context, Entity, Task};
use music::equalizer::autoeq::{self, Headphone};

use crate::{Equalizer, Io, Outcome, Toast, Toasts};

/// How many headphones a search lists at most.
const RESULTS: usize = 8;

/// The search over AutoEQ's headphones: the list, fetched the first time a search needs it, and
/// what the query in the field finds in it.
pub struct Headphones {
    equalizer: Entity<Equalizer>,
    list: Option<Arc<Vec<Headphone>>>,
    query: String,
    found: Vec<Headphone>,
    /// Whether the list could not be had, so the field says so rather than finding nothing.
    unreachable: bool,
    fetch: Option<Task<()>>,
    apply: Option<Task<()>>,
}

impl Headphones {
    pub fn new(equalizer: Entity<Equalizer>) -> Self {
        Self {
            equalizer,
            list: None,
            query: String::new(),
            found: Vec::new(),
            unreachable: false,
            fetch: None,
            apply: None,
        }
    }

    /// What the last query found.
    pub fn found(&self) -> &[Headphone] {
        &self.found
    }

    /// Whether the list is on its way, so a query has nothing to find in yet.
    pub fn loading(&self) -> bool {
        self.fetch.is_some()
    }

    /// Whether the list could not be fetched.
    pub fn unreachable(&self) -> bool {
        self.unreachable
    }

    /// Looks for `query` among the headphones, fetching the list first when it is not here yet.
    pub fn search(&mut self, query: &str, cx: &mut Context<Self>) {
        query.clone_into(&mut self.query);
        match &self.list {
            Some(list) => {
                self.found = autoeq::search(list, &self.query, RESULTS);
                cx.notify();
            }
            None if self.fetch.is_none() && !query.trim().is_empty() => self.load(cx),
            None => {}
        }
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let kept = storage::folders::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("digida")
            .join("autoeq-index.md");
        let fetch = Io::global(cx).spawn(async move {
            if let Some(folder) = kept.parent() {
                std::fs::create_dir_all(folder).ok();
            }
            autoeq::index(&kept).await
        });
        self.fetch = Some(cx.spawn(async move |this, cx| {
            let list = fetch.await;
            this.update(cx, |this, cx| {
                this.fetch = None;
                match list {
                    Ok(Ok(list)) => {
                        this.unreachable = false;
                        this.found = autoeq::search(&list, &this.query, RESULTS);
                        this.list = Some(Arc::new(list));
                    }
                    Ok(Err(error)) => {
                        log::warn!("equalizer: cannot fetch the AutoEQ list: {error:#}");
                        this.unreachable = true;
                    }
                    Err(error) => {
                        log::warn!("equalizer: the AutoEQ fetch stopped: {error:#}");
                        this.unreachable = true;
                    }
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Fetches the correction for `headphone` and plays it as a new profile named after it.
    pub fn apply(&mut self, headphone: Headphone, cx: &mut Context<Self>) {
        let fetch = Io::global(cx).spawn({
            let headphone = headphone.clone();
            async move { autoeq::profile(&headphone).await }
        });
        self.apply = Some(cx.spawn(async move |this, cx| {
            let text = fetch.await;
            this.update(cx, |this, cx| {
                this.apply = None;
                match text {
                    Ok(Ok(text)) => {
                        let imported = this.equalizer.update(cx, |equalizer, cx| {
                            equalizer.import(&headphone.name, &text, cx)
                        });
                        let toast = match imported {
                            Ok(_) => Toast::new(Outcome::Done, "equalizer-headphone-applied")
                                .name(headphone.name.clone()),
                            Err(_) => Toast::new(Outcome::Failed, "equalizer-headphone-failed"),
                        };
                        Toasts::post(toast, cx);
                    }
                    Ok(Err(error)) => {
                        log::warn!("equalizer: cannot fetch {}: {error:#}", headphone.name);
                        Toasts::show(Outcome::Failed, "equalizer-headphone-failed", cx);
                    }
                    Err(error) => {
                        log::warn!("equalizer: the AutoEQ fetch stopped: {error:#}");
                        Toasts::show(Outcome::Failed, "equalizer-headphone-failed", cx);
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }
}
