use std::collections::BTreeSet;

use anyhow::{Context as _, Result};
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use serenity::all::{ChannelId, Context, CreateEmbed, CreateEmbedFooter, CreateMessage};

use crate::db::Db;

#[derive(Debug, Clone)]
pub struct CveItem {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub os_family: String,
    pub url: String,
    pub published_at: String,
    pub cvss: Option<f64>,
    pub attack_vector: Option<String>,
    pub network_signal: String,
}

#[derive(Debug, Deserialize)]
struct NvdResponse {
    #[serde(default)]
    vulnerabilities: Vec<NvdVulnerability>,
}

#[derive(Debug, Deserialize)]
struct NvdVulnerability {
    cve: NvdCve,
}

#[derive(Debug, Deserialize)]
struct NvdCve {
    id: String,
    published: String,
    #[serde(default)]
    descriptions: Vec<NvdDescription>,
    #[serde(default)]
    references: Vec<NvdReference>,
    #[serde(default)]
    metrics: Option<NvdMetrics>,
}

#[derive(Debug, Deserialize)]
struct NvdDescription {
    lang: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct NvdReference {
    #[serde(default)]
    url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct NvdMetrics {
    #[serde(default, rename = "cvssMetricV31")]
    v31: Vec<NvdMetric>,
    #[serde(default, rename = "cvssMetricV30")]
    v30: Vec<NvdMetric>,
    #[serde(default, rename = "cvssMetricV2")]
    v2: Vec<NvdMetric>,
}

#[derive(Debug, Clone, Deserialize)]
struct NvdMetric {
    #[serde(rename = "cvssData")]
    cvss_data: NvdCvssData,
}

#[derive(Debug, Clone, Deserialize)]
struct NvdCvssData {
    #[serde(default, rename = "baseScore")]
    base_score: Option<f64>,
    #[serde(default, rename = "attackVector")]
    attack_vector: Option<String>,
}

pub async fn fetch_today(http: &Client) -> Result<Vec<CveItem>> {
    let end = Utc::now();
    let start = end
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("valid UTC midnight")
        .and_utc();
    let start = start.format("%Y-%m-%dT%H:%M:%S.000Z").to_string();
    let end = end.format("%Y-%m-%dT%H:%M:%S.000Z").to_string();

    let response = http
        .get("https://services.nvd.nist.gov/rest/json/cves/2.0")
        .query(&[
            ("pubStartDate", start.as_str()),
            ("pubEndDate", end.as_str()),
            ("resultsPerPage", "2000"),
        ])
        .send()
        .await?
        .error_for_status()
        .context("NVD API returned an error")?
        .json::<NvdResponse>()
        .await?;

    let mut items = Vec::new();
    for vuln in response.vulnerabilities {
        let description = vuln
            .cve
            .descriptions
            .iter()
            .find(|desc| desc.lang == "en")
            .or_else(|| vuln.cve.descriptions.first())
            .map(|desc| desc.value.trim().to_string())
            .unwrap_or_default();

        let evidence = format!(
            "{} {}",
            description,
            vuln.cve
                .references
                .iter()
                .map(|reference| reference.url.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        );

        let os_family = classify_os(&evidence);
        if os_family.is_empty() {
            continue;
        }

        let metrics = vuln.cve.metrics.clone();
        let attack_vector = best_attack_vector(metrics.as_ref());
        let network_signal = network_signal(&description, attack_vector.as_deref());
        if network_signal.is_empty() {
            continue;
        }

        items.push(CveItem {
            id: vuln.cve.id.clone(),
            title: truncate(&description, 360),
            summary: summarize_cve(&description),
            os_family: os_family.join(", "),
            url: format!("https://nvd.nist.gov/vuln/detail/{}", vuln.cve.id),
            published_at: vuln.cve.published,
            cvss: best_cvss(metrics.as_ref()),
            attack_vector,
            network_signal,
        });
    }

    items.sort_by(|a, b| b.published_at.cmp(&a.published_at));
    items.dedup_by(|a, b| a.id == b.id);
    Ok(items)
}

pub async fn store_new(db: &Db, items: &[CveItem]) -> Result<Vec<CveItem>> {
    let mut fresh = Vec::new();
    for item in items {
        let inserted = db
            .insert_seen(
                "cve",
                &item.id,
                &item.summary,
                &item.os_family,
                &item.url,
                &item.published_at,
            )
            .await?;

        if inserted {
            fresh.push(item.clone());
        }
    }
    Ok(fresh)
}

pub async fn send_cves(ctx: &Context, channel_id: u64, items: &[CveItem]) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }

    for chunk in items.chunks(10) {
        let color = chunk
            .iter()
            .filter_map(|item| item.cvss)
            .max_by(|a, b| a.total_cmp(b))
            .map(severity_color)
            .unwrap_or(0xd64045);

        let mut embed = CreateEmbed::new()
            .title("Network CVEs Today")
            .description("Strict filter: OS must be Linux, Windows, macOS, or BSD; attack must be network/remote related.")
            .color(color)
            .footer(CreateEmbedFooter::new(format!("{} CVE(s)", chunk.len())));

        for item in chunk {
            let score = item
                .cvss
                .map(|score| format!("CVSS {score:.1}"))
                .unwrap_or_else(|| "CVSS n/a".to_string());
            embed = embed.field(
                format!("{} | {}", item.id, severity_label(item.cvss)),
                format!(
                    "[NVD]({}) | {} | {} | {}\nSummary: {}\nSignal: {}",
                    item.url,
                    item.os_family,
                    score,
                    item.attack_vector.as_deref().unwrap_or("vector n/a"),
                    item.summary,
                    item.network_signal
                ),
                false,
            );
        }

        ChannelId::new(channel_id)
            .send_message(&ctx.http, CreateMessage::new().embed(embed))
            .await?;
    }

    Ok(())
}

fn classify_os(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut families = BTreeSet::new();

    if contains_any(
        &lower,
        &[
            "linux", "ubuntu", "debian", "red hat", "rhel", "suse", "fedora",
        ],
    ) {
        families.insert("Linux".to_string());
    }
    if contains_any(&lower, &["windows"]) {
        families.insert("Windows".to_string());
    }
    if contains_any(&lower, &["macos", "mac os", "darwin"]) {
        families.insert("macOS".to_string());
    }
    if contains_any(
        &lower,
        &["bsd", "freebsd", "openbsd", "netbsd", "dragonflybsd"],
    ) {
        families.insert("BSD".to_string());
    }

    families.into_iter().collect()
}

fn network_signal(description: &str, attack_vector: Option<&str>) -> String {
    if attack_vector
        .map(|value| value.eq_ignore_ascii_case("NETWORK"))
        .unwrap_or(false)
    {
        return "CVSS attack vector NETWORK".to_string();
    }

    let lower = description.to_ascii_lowercase();
    let signals = [
        ("remote code execution", "remote code execution"),
        ("remote attacker", "remote attacker"),
        ("network", "network exposure"),
        ("adjacent network", "adjacent network"),
        ("tcp", "TCP"),
        ("udp", "UDP"),
        ("http", "HTTP"),
        ("https", "HTTPS"),
        ("dns", "DNS"),
        ("dhcp", "DHCP"),
        ("ssh", "SSH"),
        ("tls", "TLS"),
        ("smb", "SMB"),
        ("nfs", "NFS"),
        ("kerberos", "Kerberos"),
        ("packet", "packet parsing"),
        ("socket", "socket handling"),
        ("unauthenticated", "unauthenticated access"),
        ("crafted request", "crafted request"),
    ];

    signals
        .iter()
        .find_map(|(needle, label)| lower.contains(needle).then(|| (*label).to_string()))
        .unwrap_or_default()
}

fn best_cvss(metrics: Option<&NvdMetrics>) -> Option<f64> {
    metrics.and_then(|metrics| {
        metrics
            .v31
            .iter()
            .chain(metrics.v30.iter())
            .chain(metrics.v2.iter())
            .filter_map(|metric| metric.cvss_data.base_score)
            .max_by(|a, b| a.total_cmp(b))
    })
}

fn best_attack_vector(metrics: Option<&NvdMetrics>) -> Option<String> {
    metrics.and_then(|metrics| {
        metrics
            .v31
            .iter()
            .chain(metrics.v30.iter())
            .chain(metrics.v2.iter())
            .find_map(|metric| metric.cvss_data.attack_vector.clone())
    })
}

fn summarize_cve(description: &str) -> String {
    let first_sentence = description
        .split_terminator(['.', ';'])
        .find(|part| !part.trim().is_empty())
        .unwrap_or(description)
        .trim();

    truncate(first_sentence, 260)
}

fn severity_label(score: Option<f64>) -> &'static str {
    match score {
        Some(score) if score >= 9.0 => "Critical",
        Some(score) if score >= 7.0 => "High",
        Some(score) if score >= 4.0 => "Medium",
        Some(_) => "Low",
        None => "Unknown",
    }
}

fn severity_color(score: f64) -> u32 {
    if score >= 9.0 {
        0xb00020
    } else if score >= 7.0 {
        0xd64045
    } else if score >= 4.0 {
        0xf6ae2d
    } else {
        0x2f9e44
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }

    let mut output = value
        .chars()
        .take(max.saturating_sub(3))
        .collect::<String>();
    output.push_str("...");
    output
}
