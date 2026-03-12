//! Rule-based text classification engine.
//!
//! Classifies text into categories (Transaction, Notification, Spam, etc.)
//! using keyword matching, pattern detection, and weighted scoring. The primary
//! use case is detecting financial transactions in SMS and notification text.

use crate::common;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Text classification categories.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TextCategory {
    Transaction,
    Notification,
    Spam,
    Personal,
    Authentication,
    Alert,
    Marketing,
    Unknown,
}

impl TextCategory {
    fn as_str(&self) -> &'static str {
        match self {
            TextCategory::Transaction => "Transaction",
            TextCategory::Notification => "Notification",
            TextCategory::Spam => "Spam",
            TextCategory::Personal => "Personal",
            TextCategory::Authentication => "Authentication",
            TextCategory::Alert => "Alert",
            TextCategory::Marketing => "Marketing",
            TextCategory::Unknown => "Unknown",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "Transaction" => TextCategory::Transaction,
            "Notification" => TextCategory::Notification,
            "Spam" => TextCategory::Spam,
            "Personal" => TextCategory::Personal,
            "Authentication" => TextCategory::Authentication,
            "Alert" => TextCategory::Alert,
            "Marketing" => TextCategory::Marketing,
            _ => TextCategory::Unknown,
        }
    }
}

/// A classification rule with keywords, simple patterns, and a weight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationRule {
    pub category: TextCategory,
    pub keywords: Vec<String>,
    pub patterns: Vec<String>,
    pub weight: f64,
}

/// Result of classifying a piece of text.
#[derive(Debug, Serialize)]
struct ClassificationResult {
    category: String,
    confidence: f64,
    matched_rules: Vec<String>,
}

// ---------------------------------------------------------------------------
// Built-in rules
// ---------------------------------------------------------------------------

fn built_in_rules() -> Vec<ClassificationRule> {
    vec![
        // Transaction detection (primary use case)
        ClassificationRule {
            category: TextCategory::Transaction,
            keywords: vec![
                "credit".into(),
                "debit".into(),
                "transfer".into(),
                "received".into(),
                "sent".into(),
                "payment".into(),
                "transaction".into(),
                "balance".into(),
                "withdrawal".into(),
                "deposit".into(),
                "refund".into(),
                "purchase".into(),
                "charge".into(),
                "paid".into(),
                "account".into(),
            ],
            patterns: vec![
                "NGN".into(),
                "USD".into(),
                "GBP".into(),
                "EUR".into(),
                "KES".into(),
                "GHS".into(),
                "$".into(),
                "N".into(),
                "\u{00a3}".into(),
                "\u{20ac}".into(),
            ],
            weight: 1.5,
        },
        // Bank-specific patterns for Nigerian banks
        ClassificationRule {
            category: TextCategory::Transaction,
            keywords: vec![
                "gtbank".into(),
                "firstbank".into(),
                "zenith".into(),
                "access bank".into(),
                "uba".into(),
                "stanbic".into(),
                "fidelity".into(),
                "sterling".into(),
                "wema".into(),
                "polaris".into(),
                "keystone".into(),
                "fcmb".into(),
                "ecobank".into(),
                "heritage".into(),
                "union bank".into(),
                "opay".into(),
                "palmpay".into(),
                "kuda".into(),
                "moniepoint".into(),
            ],
            patterns: vec![],
            weight: 1.2,
        },
        // Authentication / OTP
        ClassificationRule {
            category: TextCategory::Authentication,
            keywords: vec![
                "otp".into(),
                "verification".into(),
                "verify".into(),
                "code".into(),
                "pin".into(),
                "token".into(),
                "one-time".into(),
                "one time".into(),
                "2fa".into(),
                "two-factor".into(),
                "login".into(),
                "sign in".into(),
                "authentication".into(),
                "confirm".into(),
            ],
            patterns: vec![],
            weight: 1.3,
        },
        // Spam
        ClassificationRule {
            category: TextCategory::Spam,
            keywords: vec![
                "won".into(),
                "winner".into(),
                "prize".into(),
                "congratulations".into(),
                "lottery".into(),
                "free".into(),
                "claim".into(),
                "urgent".into(),
                "click here".into(),
                "act now".into(),
                "limited time".into(),
                "no cost".into(),
                "risk free".into(),
                "guaranteed".into(),
            ],
            patterns: vec![],
            weight: 1.0,
        },
        // Alert
        ClassificationRule {
            category: TextCategory::Alert,
            keywords: vec![
                "warning".into(),
                "alert".into(),
                "low balance".into(),
                "insufficient".into(),
                "expired".into(),
                "suspended".into(),
                "blocked".into(),
                "security".into(),
                "unusual activity".into(),
                "unauthorized".into(),
            ],
            patterns: vec![],
            weight: 1.1,
        },
        // Marketing
        ClassificationRule {
            category: TextCategory::Marketing,
            keywords: vec![
                "offer".into(),
                "discount".into(),
                "sale".into(),
                "promo".into(),
                "deal".into(),
                "off".into(),
                "subscribe".into(),
                "unsubscribe".into(),
                "shop".into(),
                "buy".into(),
                "exclusive".into(),
                "special".into(),
                "save".into(),
            ],
            patterns: vec![
                "%".into(),
            ],
            weight: 0.9,
        },
        // Notification (generic)
        ClassificationRule {
            category: TextCategory::Notification,
            keywords: vec![
                "shipped".into(),
                "delivered".into(),
                "order".into(),
                "tracking".into(),
                "scheduled".into(),
                "reminder".into(),
                "appointment".into(),
                "update".into(),
                "status".into(),
                "confirmed".into(),
            ],
            patterns: vec![],
            weight: 0.8,
        },
        // Personal
        ClassificationRule {
            category: TextCategory::Personal,
            keywords: vec![
                "hey".into(),
                "hello".into(),
                "hi".into(),
                "how are you".into(),
                "miss you".into(),
                "love".into(),
                "thanks".into(),
                "thank you".into(),
                "sorry".into(),
                "please".into(),
                "good morning".into(),
                "good night".into(),
            ],
            patterns: vec![],
            weight: 0.7,
        },
    ]
}

// ---------------------------------------------------------------------------
// Classification engine
// ---------------------------------------------------------------------------

/// Load custom rules from plugin state.
fn load_custom_rules() -> Vec<ClassificationRule> {
    let state = common::get_state("custom_rules").ok().flatten();
    match state {
        Some(json) => serde_json::from_str(&json).unwrap_or_default(),
        None => vec![],
    }
}

/// Get all rules (built-in + custom).
fn all_rules() -> Vec<ClassificationRule> {
    let mut rules = built_in_rules();
    rules.extend(load_custom_rules());
    rules
}

/// Score text against a single rule. Returns (score, matched_keywords).
fn score_rule(text_lower: &str, rule: &ClassificationRule) -> (f64, Vec<String>) {
    let mut matches = Vec::new();
    let mut raw_score = 0.0;

    // Keyword matching.
    for keyword in &rule.keywords {
        if text_lower.contains(&keyword.to_lowercase()) {
            matches.push(keyword.clone());
            raw_score += 1.0;
        }
    }

    // Pattern matching (simple substring for currency symbols and codes).
    for pattern in &rule.patterns {
        if text_lower.contains(&pattern.to_lowercase()) {
            matches.push(format!("pattern:{pattern}"));
            raw_score += 0.5;
        }
    }

    // Apply weight.
    let weighted = raw_score * rule.weight;
    (weighted, matches)
}

/// Classify a piece of text against all rules.
fn classify(text: &str) -> ClassificationResult {
    let text_lower = text.to_lowercase();
    let rules = all_rules();

    let mut best_category = TextCategory::Unknown;
    let mut best_score: f64 = 0.0;
    let mut best_matches = Vec::new();
    let mut total_score: f64 = 0.0;

    for rule in &rules {
        let (score, matches) = score_rule(&text_lower, rule);
        if score > 0.0 {
            total_score += score;
            if score > best_score {
                best_score = score;
                best_category = rule.category.clone();
                best_matches = matches;
            }
        }
    }

    // Confidence: ratio of best score to total, clamped to [0, 1].
    let confidence = if total_score > 0.0 {
        (best_score / total_score).min(1.0)
    } else {
        0.0
    };

    ClassificationResult {
        category: best_category.as_str().to_string(),
        confidence,
        matched_rules: best_matches,
    }
}

// ---------------------------------------------------------------------------
// Public action handlers
// ---------------------------------------------------------------------------

/// Classify arbitrary text.
///
/// Params: `{text: string}`
pub fn classify_text(params: &Value) -> Result<String, String> {
    let text = params["text"]
        .as_str()
        .ok_or("Missing required field: text")?;

    let result = classify(text);
    serde_json::to_string(&result).map_err(|e| format!("Serialization error: {e}"))
}

/// Classify an SMS message with enhanced sender context.
///
/// Params: `{from: string, body: string}`
pub fn classify_sms(params: &Value) -> Result<String, String> {
    let from = params["from"]
        .as_str()
        .ok_or("Missing required field: from")?;
    let body = params["body"]
        .as_str()
        .ok_or("Missing required field: body")?;

    // Combine sender info with body for better classification.
    // Short-code senders (numeric, < 10 digits) are often automated.
    let is_short_code = from.chars().all(|c| c.is_ascii_digit()) && from.len() < 10;

    let mut result = classify(body);

    // Boost transaction/auth confidence for short-code senders.
    if is_short_code
        && (result.category == "Transaction" || result.category == "Authentication")
    {
        result.confidence = (result.confidence * 1.15).min(1.0);
        result.matched_rules.push("sender:short_code".to_string());
    }

    // If sender contains a bank name, boost transaction confidence.
    let from_lower = from.to_lowercase();
    let bank_keywords = [
        "bank", "gtbank", "zenith", "access", "uba", "firstbank", "fidelity",
        "stanbic", "opay", "palmpay", "kuda", "moniepoint",
    ];
    for kw in &bank_keywords {
        if from_lower.contains(kw) {
            if result.category != "Transaction" {
                result.category = "Transaction".to_string();
            }
            result.confidence = (result.confidence * 1.2).min(1.0);
            result
                .matched_rules
                .push(format!("sender:bank:{kw}"));
            break;
        }
    }

    serde_json::to_string(&result).map_err(|e| format!("Serialization error: {e}"))
}

/// Classify an email message with metadata context.
///
/// Params: `{from: string, subject: string, body: string}`
pub fn classify_email(params: &Value) -> Result<String, String> {
    let from = params["from"]
        .as_str()
        .ok_or("Missing required field: from")?;
    let subject = params["subject"]
        .as_str()
        .ok_or("Missing required field: subject")?;
    let body = params["body"]
        .as_str()
        .ok_or("Missing required field: body")?;

    // Classify subject and body separately, take the higher confidence.
    let subject_result = classify(subject);
    let body_result = classify(body);

    let mut result = if subject_result.confidence >= body_result.confidence {
        subject_result
    } else {
        body_result
    };

    // Check sender domain for known patterns.
    let from_lower = from.to_lowercase();
    if from_lower.contains("noreply") || from_lower.contains("no-reply") {
        result
            .matched_rules
            .push("sender:noreply".to_string());
    }
    if from_lower.contains("bank") || from_lower.contains("transaction") {
        result.confidence = (result.confidence * 1.1).min(1.0);
        result
            .matched_rules
            .push("sender:banking_domain".to_string());
    }

    serde_json::to_string(&result).map_err(|e| format!("Serialization error: {e}"))
}

/// Extract currency amounts from text.
///
/// Params: `{text: string}`
///
/// Finds patterns like "NGN 5,000.00", "$100", "N50,000", "\u{00a3}200.50".
pub fn extract_amount(params: &Value) -> Result<String, String> {
    let text = params["text"]
        .as_str()
        .ok_or("Missing required field: text")?;

    let amounts = find_amounts(text);

    Ok(serde_json::json!({
        "amounts": amounts,
        "count": amounts.len(),
    })
    .to_string())
}

/// Extract phone numbers from text.
///
/// Params: `{text: string}`
pub fn extract_phone_number(params: &Value) -> Result<String, String> {
    let text = params["text"]
        .as_str()
        .ok_or("Missing required field: text")?;

    let phones = find_phone_numbers(text);

    Ok(serde_json::json!({
        "phoneNumbers": phones,
        "count": phones.len(),
    })
    .to_string())
}

/// Extract transaction reference numbers from text.
///
/// Params: `{text: string}`
pub fn extract_reference(params: &Value) -> Result<String, String> {
    let text = params["text"]
        .as_str()
        .ok_or("Missing required field: text")?;

    let refs = find_references(text);

    Ok(serde_json::json!({
        "references": refs,
        "count": refs.len(),
    })
    .to_string())
}

/// Add a custom classification rule (persisted via set_state).
///
/// Params: `{category: string, keywords: [string], weight?: number}`
pub fn add_custom_rule(params: &Value) -> Result<String, String> {
    let category_str = params["category"]
        .as_str()
        .ok_or("Missing required field: category")?;
    let keywords = params["keywords"]
        .as_array()
        .ok_or("Missing required field: keywords (array)")?;
    let weight = params["weight"].as_f64().unwrap_or(1.0);

    let keyword_strings: Vec<String> = keywords
        .iter()
        .filter_map(|k| k.as_str().map(String::from))
        .collect();

    let category = TextCategory::from_str(category_str);

    let rule = ClassificationRule {
        category,
        keywords: keyword_strings.clone(),
        patterns: vec![],
        weight,
    };

    // Load existing custom rules, append, and save.
    let mut custom_rules = load_custom_rules();
    custom_rules.push(rule);

    let json = serde_json::to_string(&custom_rules)
        .map_err(|e| format!("Serialization error: {e}"))?;
    common::set_state("custom_rules", &json)?;

    common::log(
        common::log_level::INFO,
        &format!(
            "Added custom rule for category '{}' with {} keywords",
            category_str,
            keyword_strings.len()
        ),
    );

    Ok(serde_json::json!({
        "status": "added",
        "category": category_str,
        "keywords": keyword_strings,
        "weight": weight,
        "totalCustomRules": custom_rules.len(),
    })
    .to_string())
}

/// List all categories with their rule counts.
pub fn get_categories(_params: &Value) -> Result<String, String> {
    let rules = all_rules();

    let categories = [
        "Transaction",
        "Notification",
        "Spam",
        "Personal",
        "Authentication",
        "Alert",
        "Marketing",
        "Unknown",
    ];

    let mut result = Vec::new();
    for cat_name in &categories {
        let cat = TextCategory::from_str(cat_name);
        let rule_count = rules
            .iter()
            .filter(|r| r.category == cat)
            .count();
        let keyword_count: usize = rules
            .iter()
            .filter(|r| r.category == cat)
            .map(|r| r.keywords.len())
            .sum();
        result.push(serde_json::json!({
            "category": cat_name,
            "ruleCount": rule_count,
            "keywordCount": keyword_count,
        }));
    }

    Ok(serde_json::json!({ "categories": result }).to_string())
}

// ---------------------------------------------------------------------------
// Extraction helpers
// ---------------------------------------------------------------------------

/// Find currency amounts in text.
///
/// Looks for currency symbols/codes followed by (or preceding) numeric values.
fn find_amounts(text: &str) -> Vec<Value> {
    let mut amounts = Vec::new();

    // Currency prefixes to look for.
    let prefixes = [
        ("NGN", "NGN"),
        ("USD", "USD"),
        ("GBP", "GBP"),
        ("EUR", "EUR"),
        ("KES", "KES"),
        ("GHS", "GHS"),
        ("$", "USD"),
        ("\u{20a6}", "NGN"), // Naira sign
        ("\u{00a3}", "GBP"), // Pound sign
        ("\u{20ac}", "EUR"), // Euro sign
        ("N", "NGN"),        // Common shorthand
    ];

    for (symbol, currency) in &prefixes {
        // Find all occurrences of this symbol.
        let mut search_start = 0;
        while let Some(pos) = text[search_start..].find(symbol) {
            let abs_pos = search_start + pos;
            let after_symbol = abs_pos + symbol.len();

            // For single-letter symbols like "N", require the next char to be
            // a digit or space+digit to avoid false positives.
            if symbol.len() == 1 && symbol.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
                // Check char before is not alphabetic (avoid matching "Not", "No", etc.)
                if abs_pos > 0 {
                    let prev_byte = text.as_bytes().get(abs_pos - 1).copied().unwrap_or(0);
                    if (prev_byte as char).is_alphabetic() {
                        search_start = after_symbol;
                        continue;
                    }
                }
            }

            // Try to parse a number after the symbol (with optional space).
            if after_symbol < text.len() {
                let remainder = &text[after_symbol..];
                let trimmed = remainder.trim_start();
                if let Some(amount) = parse_number_prefix(trimmed) {
                    let raw = format!("{}{}", symbol, trimmed.split_whitespace().next().unwrap_or(""));
                    amounts.push(serde_json::json!({
                        "amount": amount,
                        "currency": currency,
                        "raw": raw,
                    }));
                }
            }

            search_start = after_symbol;
            if search_start >= text.len() {
                break;
            }
        }
    }

    // Deduplicate by amount+currency.
    amounts.dedup_by(|a, b| a["amount"] == b["amount"] && a["currency"] == b["currency"]);
    amounts
}

/// Try to parse a number from the start of a string.
///
/// Handles formats like "5,000.00", "5000", "5,000", "5000.00".
fn parse_number_prefix(s: &str) -> Option<f64> {
    let mut num_str = String::new();
    let mut has_digit = false;
    let mut has_dot = false;

    for ch in s.chars() {
        if ch.is_ascii_digit() {
            num_str.push(ch);
            has_digit = true;
        } else if ch == ',' && has_digit {
            // Skip commas in numbers.
            continue;
        } else if ch == '.' && has_digit && !has_dot {
            num_str.push(ch);
            has_dot = true;
        } else {
            break;
        }
    }

    if has_digit {
        num_str.parse::<f64>().ok()
    } else {
        None
    }
}

/// Find phone numbers in text.
///
/// Matches common formats: +234XXXXXXXXXX, 0XXXXXXXXXX, +1XXXXXXXXXX, etc.
fn find_phone_numbers(text: &str) -> Vec<String> {
    let mut phones = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Start of a potential phone number.
        if chars[i] == '+' || chars[i].is_ascii_digit() {
            let mut digits = String::new();
            let mut raw = String::new();

            while i < len {
                let ch = chars[i];
                if ch.is_ascii_digit() || ch == '+' || ch == '-' || ch == ' ' || ch == '(' || ch == ')' {
                    raw.push(ch);
                    if ch.is_ascii_digit() || ch == '+' {
                        digits.push(ch);
                    }
                    i += 1;
                } else {
                    break;
                }
            }

            // A phone number should have 10-15 digits.
            let digit_count = digits.chars().filter(|c| c.is_ascii_digit()).count();
            if digit_count >= 10 && digit_count <= 15 {
                phones.push(raw.trim().to_string());
            }
        } else {
            i += 1;
        }
    }

    phones
}

/// Find transaction reference numbers in text.
///
/// Looks for common patterns like "TXN123456", "REF: ABC123", alphanumeric
/// strings near "reference", "ref", "txn", "transaction id".
fn find_references(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let text_lower = text.to_lowercase();

    // Look for labeled references.
    let labels = [
        "ref:", "ref :", "reference:", "reference :", "txn:", "txn :",
        "transaction id:", "transaction id :", "receipt:",
        "receipt no:", "receipt no.:",
    ];

    for label in &labels {
        if let Some(pos) = text_lower.find(label) {
            let after = &text[pos + label.len()..];
            let trimmed = after.trim_start();
            // Take the next word/token as the reference.
            let reference: String = trimmed
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if reference.len() >= 4 {
                refs.push(reference);
            }
        }
    }

    // Look for standalone patterns like "TXN123456".
    let prefixes = ["TXN", "REF", "TXNID", "RRN"];
    for prefix in &prefixes {
        let mut search_start = 0;
        while let Some(pos) = text[search_start..].find(prefix) {
            let abs_pos = search_start + pos;
            let after = &text[abs_pos..];
            let reference: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if reference.len() > prefix.len() + 2 {
                refs.push(reference);
            }
            search_start = abs_pos + prefix.len();
            if search_start >= text.len() {
                break;
            }
        }
    }

    refs.dedup();
    refs
}
