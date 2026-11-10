//! Host-owned scoring and bounded scraping; plugins only map upstream responses.
use crate::{Cancellation, PluginError, Result, ScrapeCandidate, ScrapeQuery, Source, SourceApi};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

pub fn normalize_match(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| {
            c.is_ascii_alphanumeric()
                || ('\u{3400}'..='\u{9fff}').contains(c)
                || ('\u{f900}'..='\u{faff}').contains(c)
        })
        .collect()
}
pub fn bigram_similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (normalize_match(a), normalize_match(b));
    if a.is_empty() || b.is_empty() {
        return 0.;
    }
    if a == b {
        return 1.;
    }
    let (a, b) = (a.chars().collect::<Vec<_>>(), b.chars().collect::<Vec<_>>());
    if a.len() < 2 || b.len() < 2 {
        return 0.;
    }
    let mut counts = BTreeMap::new();
    for pair in a.windows(2) {
        *counts.entry((pair[0], pair[1])).or_insert(0usize) += 1;
    }
    let mut hits = 0;
    for pair in b.windows(2) {
        if let Some(left) = counts.get_mut(&(pair[0], pair[1])) {
            if *left > 0 {
                hits += 1;
                *left -= 1;
            }
        }
    }
    2. * hits as f64 / (a.len() + b.len() - 2) as f64
}
pub fn duration_fit(a: Option<f64>, b: Option<f64>) -> f64 {
    match (a, b) {
        (Some(a), Some(b)) if a > 0. && b > 0. => ((10. - (a - b).abs()) / 8.).clamp(0., 1.),
        _ => 0.5,
    }
}
pub fn score(query: &ScrapeQuery, candidate: &ScrapeCandidate) -> f64 {
    let title = bigram_similarity(&query.title, &candidate.title);
    let duration = duration_fit(query.duration, candidate.duration);
    if let Some(artist) = query.artists.first().filter(|s| !s.trim().is_empty()) {
        let artist = candidate
            .artists
            .iter()
            .map(|a| bigram_similarity(artist, a))
            .fold(0., f64::max);
        0.5 * title + 0.3 * artist + 0.2 * duration
    } else {
        0.8 * title + 0.2 * duration
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Auto,
    Review,
    None,
}
pub fn verdict(scores: &[f64]) -> Verdict {
    match scores.first() {
        Some(best) if *best >= 0.86 && best - scores.get(1).copied().unwrap_or(0.) >= 0.08 => {
            Verdict::Auto
        }
        Some(best) if *best >= 0.55 => Verdict::Review,
        _ => Verdict::None,
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoredCandidate {
    pub source_id: String,
    pub candidate: ScrapeCandidate,
    pub score: f64,
    pub source_count: usize,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ScrapeReport {
    pub verdict: Verdict,
    pub candidates: Vec<ScoredCandidate>,
    pub errors: BTreeMap<String, String>,
}
#[derive(Clone)]
pub struct Scraper {
    permits: Arc<tokio::sync::Semaphore>,
    timeout: Duration,
}
impl Default for Scraper {
    fn default() -> Self {
        Self {
            permits: Arc::new(tokio::sync::Semaphore::new(4)),
            timeout: Duration::from_secs(8),
        }
    }
}
impl Scraper {
    pub async fn search(
        &self,
        query: ScrapeQuery,
        sources: Vec<Arc<Source>>,
        cancel: Cancellation,
    ) -> Result<ScrapeReport> {
        let mut jobs = tokio::task::JoinSet::new();
        for source in sources {
            let (query, permits, parent, timeout) = (
                query.clone(),
                self.permits.clone(),
                cancel.clone(),
                self.timeout,
            );
            jobs.spawn(async move {
                let id = source.record.source_id.clone();
                let permit = tokio::select! { biased; _ = parent.cancelled() => return (id, Err(PluginError::new("CANCELLED", "Scraping cancelled"))), permit = permits.acquire_owned() => permit.unwrap() };
                let api = SourceApi::new(source);
                let token = Cancellation::default();
                let call = api.scrape(query, Some(&token));
                tokio::pin!(call);
                let result = tokio::select! {
                    biased;
                    _ = parent.cancelled() => { token.cancel(); let _ = call.await; Err(PluginError::new("CANCELLED", "Scraping cancelled")) },
                    _ = tokio::time::sleep(timeout) => { token.cancel(); let _ = call.await; Err(PluginError::new("TIMEOUT", "Scraper exceeded 8 seconds")) },
                    result = &mut call => result,
                };
                drop(permit);
                (id, result)
            });
        }
        let (mut merged, mut errors, mut successes) = (
            BTreeMap::<String, (ScoredCandidate, std::collections::BTreeSet<String>)>::new(),
            BTreeMap::new(),
            0,
        );
        while let Some(job) = jobs.join_next().await {
            let (id, result) = job.map_err(|e| PluginError::new("PLUGIN_ERROR", e.to_string()))?;
            match result {
                Err(error) => {
                    errors.insert(id, error.code);
                }
                Ok(items) => {
                    successes += 1;
                    for candidate in items {
                        let key = serde_json::to_string(&(
                            normalize_match(&candidate.title),
                            normalize_match(
                                candidate.artists.first().map(String::as_str).unwrap_or(""),
                            ),
                            candidate.duration,
                        ))?;
                        let score = score(&query, &candidate);
                        let value = ScoredCandidate {
                            source_id: id.clone(),
                            candidate,
                            score,
                            source_count: 1,
                        };
                        let entry = merged
                            .entry(key)
                            .or_insert_with(|| (value.clone(), Default::default()));
                        entry.1.insert(id.clone());
                        if value.score > entry.0.score {
                            entry.0 = value;
                        }
                    }
                }
            }
        }
        if cancel.is_cancelled() {
            return Err(PluginError::new("CANCELLED", "Scraping cancelled"));
        }
        if successes == 0 && !errors.is_empty() {
            return Err(PluginError::new(
                "UPSTREAM_ERROR",
                serde_json::to_string(&errors)?,
            ));
        }
        let mut candidates = merged
            .into_values()
            .map(|(mut value, sources)| {
                value.source_count = sources.len();
                value
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then(b.source_count.cmp(&a.source_count))
                .then(a.source_id.cmp(&b.source_id))
        });
        Ok(ScrapeReport {
            verdict: verdict(&candidates.iter().map(|v| v.score).collect::<Vec<_>>()),
            candidates,
            errors,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_scoring_and_decision_boundaries() {
        assert_eq!(normalize_match("A-你 好！B"), "a你好b");
        assert_eq!(bigram_similarity("AAAA", "AA"), 0.5);
        assert_eq!(bigram_similarity("晴天", "晴天"), 1.);
        assert_eq!(duration_fit(None, Some(20.)), 0.5);
        assert_eq!(duration_fit(Some(20.), Some(22.)), 1.);
        assert_eq!(duration_fit(Some(20.), Some(26.)), 0.5);
        assert_eq!(duration_fit(Some(20.), Some(30.)), 0.);
        assert_eq!(verdict(&[0.9, 0.7]), Verdict::Auto);
        assert_eq!(verdict(&[0.86]), Verdict::Auto);
        assert_eq!(verdict(&[0.9, 0.85]), Verdict::Review);
        assert_eq!(verdict(&[0.55]), Verdict::Review);
        assert_eq!(verdict(&[0.549]), Verdict::None);
        assert_eq!(Scraper::default().timeout, Duration::from_secs(8));
    }
    #[tokio::test]
    async fn four_permits_bound_work_across_batches() {
        let scraper = Scraper::default();
        let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let peak = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut jobs = tokio::task::JoinSet::new();
        for _ in 0..12 {
            let (permits, active, peak) = (scraper.permits.clone(), active.clone(), peak.clone());
            jobs.spawn(async move {
                let _permit = permits.acquire_owned().await.unwrap();
                let n = active.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                peak.fetch_max(n, std::sync::atomic::Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(5)).await;
                active.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            });
        }
        while jobs.join_next().await.is_some() {}
        assert_eq!(peak.load(std::sync::atomic::Ordering::SeqCst), 4);
    }
}
