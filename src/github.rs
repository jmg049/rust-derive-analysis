use leabharlann_network::HttpClient;
use serde::Deserialize;
use tracing::{info, warn};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use rand::{rng, Rng};
use crate::RepositoryInfo;
fn jitter(ms: u64) -> Duration {
    let j = rng().random_range(0..=ms/2);
    Duration::from_millis(ms + j)
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

#[derive(Debug, Deserialize)]
struct GitHubSearchResponse {
    total_count: u32,
    incomplete_results: bool,
    items: Vec<GitHubRepository>,
}

#[derive(Debug, Deserialize)]
struct GitHubRepository {
    id: u64,
    name: String,
    full_name: String,
    clone_url: String,
    language: Option<String>,
    stargazers_count: u32,
    size: u32,
    default_branch: String,
}

#[derive(Clone)]
pub struct GitHubClient {
    client: HttpClient,
    token: Option<String>,
}

impl GitHubClient {
    pub fn new(token: Option<String>) -> Self {
        if token.is_some() {
            info!("Token detected (authenticated GitHub requests enabled)");
        } else {
            warn!("GITHUB_TOKEN not set — rate limits will be very restrictive");
        }
        Self { client: HttpClient::new(), token }
    }

    pub async fn search_rust_repositories(
        &self,
        limit: usize,
        min_stars: u32,
    ) -> Result<Vec<RepositoryInfo>, Box<dyn std::error::Error>> {
        info!("Searching for Rust repositories on GitHub (limit: {}, min_stars: {})", limit, min_stars);

        let desired = limit.min(1000);                  // hard cap: GitHub search only returns first 1000
        let per_page = 100.min(desired);
        let max_pages = ((desired + per_page - 1) / per_page).min(10); // never exceed page 10

        let mut repositories = Vec::with_capacity(desired);
        let mut page = 1usize;

        while repositories.len() < desired && page <= max_pages {
            let url = format!(
                "https://api.github.com/search/repositories?q=language:rust+stars:>={}+sort:stars+size:>10&sort=stars&order=desc&page={}&per_page={}",
                min_stars, page, per_page
            );

            // Retry with exponential backoff on 403/429
            let mut attempt = 0u32;
            let search_resp: GitHubSearchResponse = loop {
                // Build request with headers (must be rebuilt for each retry)
                let mut req = self.client.get(&url)
                    .header("Accept", "application/vnd.github+json")
                    .header("User-Agent", "rust-derive-analysis/1.0 (+https://github.com/your/name)")
                    .header("X-GitHub-Api-Version", "2022-11-28");
                if let Some(ref t) = self.token {
                    // Either "token" or "Bearer" works; GitHub recommends Bearer for fine-grained tokens
                    req = req.header("Authorization", format!("Bearer {}", t));
                }

                let resp = req.send().await?;
                let status = resp.status();
                // Try to expose headers from your HttpClient; adjust if your API differs
                let headers = resp.headers().clone();

                if status.is_success() {
                    // optional: sleep if Remaining==0 until Reset
                    if let (Some(rem), Some(reset)) = (headers.get("X-RateLimit-Remaining"), headers.get("X-RateLimit-Reset")) {
                        if rem.to_str().ok().and_then(|s| s.parse::<i64>().ok()) == Some(0) {
                            if let Ok(ts) = reset.to_str().unwrap_or("").parse::<u64>() {
                                let wait = ts.saturating_sub(now_secs());
                                warn!("Rate limit exhausted; sleeping {}s until reset", wait);
                                tokio::time::sleep(jitter(wait * 1000)).await;
                            }
                        }
                    }
                    // Parse JSON
                    break resp.json().await?;
                }

                if status.as_u16() == 403 || status.as_u16() == 429 {
                    attempt += 1;
                    // Honour Retry-After if present
                    if let Some(ra) = headers.get("Retry-After") {
                        if let Ok(sec) = ra.to_str().unwrap_or("").parse::<u64>() {
                            warn!("{} received; Retry-After={}s. Backing off.", status, sec);
                            tokio::time::sleep(jitter(sec * 1000)).await;
                            continue;
                        }
                    }
                    let backoff = 2u64.saturating_pow(attempt.min(6)); // 2,4,8,16,32,64
                    warn!("{} received; exponential backoff {}s (attempt {})", status, backoff, attempt);
                    tokio::time::sleep(jitter(backoff * 1000)).await;
                    continue;
                }

                // For other errors, include body to aid debugging
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("Search failed ({}): {}", status, body).into());
            };

            let items_len = search_resp.items.len();
            info!("Found {} repositories on page {}", items_len, page);

            for repo in search_resp.items {
                if repositories.len() >= desired { break; }
                // Rely on search query for language; keep your "substantial content" gate if desired
                if repo.size > 10 {
                    repositories.push(RepositoryInfo {
                        name: repo.name,
                        full_name: repo.full_name,
                        clone_url: repo.clone_url,
                        language: repo.language,
                        stars: repo.stargazers_count,
                    });
                }
            }

            // Stop early if this page was short (end of results for the query)
            if items_len < per_page { break; }

            page += 1;

            // Throttle search endpoint even when authenticated (~30 req/min budget)
            tokio::time::sleep(jitter(2200)).await; // ~2.2s + jitter
        }

        info!("Collected {} Rust repositories (requested {} <= 1000 cap)", repositories.len(), desired);
        Ok(repositories)
    }
}
