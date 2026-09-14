//! "Ask the Pen Market": the assistant API. The model (CrossGen's hosted OpenAI-compatible endpoint)
//! only talks; every number comes from `engine` tools run here. No model, no answer: the page says so.

use crate::app::{AppError, State};
use crate::db;
use crate::engine::{self, Criteria};
use crate::money::fmt_dollars;
use crate::security;
use axum::extract::{ConnectInfo, State as AxState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::SocketAddr;

#[derive(Deserialize)]
pub struct ChatBody {
    pub messages: Vec<Msg>,
}
#[derive(Deserialize, Serialize, Clone)]
pub struct Msg {
    pub role: String,
    pub content: String,
}
#[derive(Serialize)]
pub struct ToolResult {
    pub name: String,
    pub args: Value,
    pub result: Value,
}
#[derive(Serialize)]
pub struct ChatReply {
    pub text: String,
    #[serde(rename = "toolResults")]
    pub tool_results: Vec<ToolResult>,
}

pub fn system_prompt(state: &State) -> String {
    let cat = state.catalog();
    let s = engine::stats(&cat);
    let cats: Vec<String> = s.by_category.iter().map(|c| format!("{} {}", c.count, c.name)).collect();
    format!(
        "You are Ask the Pen Market, the AI guide on ThePenMarket.com, Nathaniel Cerf's vintage pen shop (online since 2007, Norwich, Connecticut). You are an AI, not Nathaniel; say so if asked. Nathaniel restores and sells vintage fountain pens and pre-owned luxury pens, one of each, repairs vintage pens, buys and consigns collections, and runs the Trading Post classifieds ($5 a listing, $125 a year for Unlimited Posts). The blog is Drippy Musings.
Voice: plain, warm, a little wry, collector to collector, like his blog. Short messages: at most three short sentences or a short list. One question at a time. Plain text only: no markdown, no asterisks, no headings, no emojis, no em dashes.
The catalog right now: {live} items in stock ({cats}), {min} to {max}, {brands} brands with stock. Never repeat these counts from memory later; call the tools.
RULES:
- NEVER state a price, count, length, date, availability or whether a repair is in scope from memory. Only report what your tools return. If you do not have a tool result yet, ask for the missing detail or call the tool.
- To find pens, call find_pens as soon as you have any one of: a budget, a brand, an era, a nib, a filling system, a category, or a few keywords. Do not ask more than one clarifying question before searching. Parse sensibly: \"under 200\" is max_price 200; \"1930s\" is era 1930s; \"flex\" is nib flex.
- After find_pens, name the top one or two pens with their price and one reason, mention how many matched, and offer to narrow or to show the page. The page shows cards for the results; do not list every field.
- For a specific pen (a SKU, a model name), call pen_details.
- For \"can you fix my X\" call repair_scope with the visitor's words. For \"do you buy X\" or \"I want to sell\" call sell_triage. For nib width questions call nib_fact. For return or guarantee dates call guarantee_dates with the delivery date.
- Estimates for repairs and offers for collections come from Nathaniel, never from you. Do not invent turnaround times, repair prices or offers.
- Authenticity: you cannot authenticate a pen. Point to his fake-Montblanc posts and the description on the listing.
- Checkout is not connected in this preview build; say a visitor can contact the shop to buy, and never pretend to take payment.
- Off topic: one sentence, then back to pens.
- When the visitor wants a repair estimate, wants to sell, wants a pen over $1,000, or asks for a person, call handoff with a two-sentence summary; then give the phone (847) 708-5062 and info@thepenmarket.com.
- HARD RULE: if the visitor's first message already contains a budget or a pen description, your first action is find_pens, not a question.",
        live = s.live_total, cats = cats.join(", "), min = fmt_dollars(s.price_min_cents), max = fmt_dollars(s.price_max_cents), brands = s.brands_with_stock
    )
}

pub fn tools() -> Vec<Value> {
    let f = |name: &str, description: &str, params: Value| json!({"type": "function", "function": {"name": name, "description": description, "parameters": params}});
    vec![
        f("find_pens", "Search the live catalog and rank matching pens with prices and reasons. Call with whatever the visitor gave; every argument is optional.", json!({"type": "object", "properties": {
            "max_price": {"type": "number", "description": "dollars"}, "min_price": {"type": "number"},
            "category": {"type": "string", "description": "vintage, pre-owned, pencil, inkwell, camera"},
            "brand": {"type": "string"}, "era": {"type": "string", "description": "e.g. 1920s, 1930-1939, modern"},
            "nib": {"type": "string", "description": "extra fine, fine, medium, broad, stub, oblique, flex, semi-flex"},
            "mechanism": {"type": "string", "description": "lever, button, vacumatic, snorkel, touchdown, aerometric, piston, cartridge, eyedropper"},
            "flex": {"type": "boolean"}, "keywords": {"type": "string", "description": "free words such as celluloid, oversize, red"},
            "include_sold": {"type": "boolean"}, "limit": {"type": "integer"}}, "required": []})),
        f("pen_details", "Full details of one pen by SKU, slug or name.", json!({"type": "object", "properties": {"key": {"type": "string"}}, "required": ["key"]})),
        f("repair_scope", "Whether a described pen or filling system is on his published repair list, with the shipping guidelines.", json!({"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]})),
        f("sell_triage", "Whether he is looking for the pens a visitor wants to sell, and the options (cash, consignment).", json!({"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]})),
        f("guarantee_dates", "Return-by and repair-promise dates from the day a pen was received (YYYY-MM-DD).", json!({"type": "object", "properties": {"received_on": {"type": "string"}}, "required": ["received_on"]})),
        f("nib_fact", "Line width and notes for a nib grade.", json!({"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]})),
        f("handoff", "Record a summary for Nathaniel and show the visitor the human contact card.", json!({"type": "object", "properties": {"summary": {"type": "string"}}, "required": ["summary"]})),
    ]
}

/// Run one tool. Returns (what the model sees, what the page renders).
pub async fn run_tool(state: &State, name: &str, args: &Value) -> (Value, Option<Value>) {
    let cat = state.catalog();
    let s = |k: &str| args.get(k).and_then(|v| v.as_str()).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    match name {
        "find_pens" => {
            let c: Criteria = serde_json::from_value(args.clone()).unwrap_or_default();
            let r = engine::find_pens(&cat, &c);
            let v = serde_json::to_value(&r).unwrap_or(Value::Null);
            (v.clone(), Some(v))
        }
        "pen_details" => match s("key").and_then(|k| engine::pen_details(&cat, &k).cloned()) {
            Some(p) => {
                let v = json!({"sku": p.sku, "title": p.title, "url": p.url(), "price": crate::money::fmt_cents(p.effective_cents()), "was_price": p.sale_price_cents.map(|_| crate::money::fmt_cents(p.price_cents)), "status": p.status, "brand": p.brand, "era": p.era, "nib": p.nib, "mechanism": p.mechanism, "repairable_system": p.repairable, "length_cm": p.length_mm.map(|mm| format!("{:.1}", mm / 10.0)), "summary": p.summary, "image": p.image});
                (v.clone(), Some(json!({"matches": [{"sku": p.sku, "title": p.title, "short_title": p.short_title, "url": p.url(), "image": p.image, "price": crate::money::fmt_cents(p.effective_cents()), "was_price": p.sale_price_cents.map(|_| crate::money::fmt_cents(p.price_cents)), "brand": p.brand, "era": p.era, "nib": p.nib, "mechanism": p.mechanism, "status": p.status, "reasons": [], "summary": p.summary}], "total_matching": 1, "shown": 1, "note": ""})))
            }
            None => (json!({"error": "no pen matches that key; ask for the SKU or the model name as it appears on the site"}), None),
        },
        "repair_scope" => {
            let r = engine::repair_scope(&s("text").unwrap_or_default());
            let v = serde_json::to_value(&r).unwrap_or(Value::Null);
            (v.clone(), Some(v))
        }
        "sell_triage" => {
            let r = engine::sell_triage(&s("text").unwrap_or_default());
            let v = serde_json::to_value(&r).unwrap_or(Value::Null);
            (v.clone(), Some(v))
        }
        "guarantee_dates" => match s("received_on").and_then(|d| d.parse::<chrono::NaiveDate>().ok()) {
            Some(d) => {
                let v = serde_json::to_value(engine::guarantee_dates(d)).unwrap_or(Value::Null);
                (v.clone(), Some(v))
            }
            None => (json!({"error": "received_on must be a date like 2026-09-14; ask the visitor when the pen arrived"}), None),
        },
        "nib_fact" => match s("name").and_then(|n| engine::nib_fact(&n)) {
            Some(f) => (serde_json::to_value(f).unwrap_or(Value::Null), None),
            None => (json!({"error": "unknown nib grade; the grades are extra-fine, fine, medium, broad, BB, stub, oblique, accountant, semi-flexible, flexible"}), None),
        },
        "handoff" => {
            let summary = s("summary").unwrap_or_default();
            let _ = db::save_form(&state.pool, "guide_handoff", json!({"summary": summary}), None).await;
            (json!({"ok": true, "phone": "(847) 708-5062", "email": "info@thepenmarket.com"}), Some(json!({"summary": summary})))
        }
        _ => (json!({"error": "unknown tool"}), None),
    }
}

/// The catalog snapshot the artifact inlines (public catalog data only).
pub async fn catalog_json(AxState(state): AxState<State>) -> Response {
    let cat = state.catalog();
    ([(axum::http::header::CACHE_CONTROL, "public, max-age=600")], Json(serde_json::to_value(&*cat).unwrap_or(Value::Null))).into_response()
}

pub async fn health(AxState(state): AxState<State>) -> Response {
    let cat = state.catalog();
    if state.cfg.guide_ai_url.is_empty() || state.cfg.guide_ai_key.is_empty() {
        return Json(json!({"backend": "none", "catalog": cat.pens.len()})).into_response();
    }
    let host = state.cfg.guide_ai_url.trim_start_matches("https://").trim_start_matches("http://").split('/').next().unwrap_or("").to_string();
    Json(json!({"backend": "openai-compatible", "model": state.cfg.guide_ai_model, "host": host, "catalog": cat.pens.len(), "generated_at": cat.generated_at})).into_response()
}

fn err(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({"error": msg}))).into_response()
}

pub async fn chat(AxState(state): AxState<State>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, body: Result<Json<ChatBody>, axum::extract::rejection::JsonRejection>) -> Result<Response, AppError> {
    if state.cfg.guide_ai_url.is_empty() || state.cfg.guide_ai_key.is_empty() {
        return Ok(err(StatusCode::SERVICE_UNAVAILABLE, "no model configured"));
    }
    if !state.guide_limiter.allow(security::client_ip(&headers, peer.ip())) {
        return Ok(err(StatusCode::TOO_MANY_REQUESTS, "too many requests, try again in a minute"));
    }
    let Json(body) = match body {
        Ok(b) => b,
        Err(_) => return Ok(err(StatusCode::BAD_REQUEST, "body must be JSON {messages:[{role,content}]}")),
    };
    let messages: Vec<Msg> = body.messages.into_iter().filter(|m| (m.role == "user" || m.role == "assistant") && !m.content.trim().is_empty()).map(|m| Msg { role: m.role, content: m.content.chars().take(2000).collect() }).collect();
    let messages: Vec<Msg> = messages.into_iter().rev().take(24).collect::<Vec<_>>().into_iter().rev().collect();
    if messages.last().map(|m| m.role != "user").unwrap_or(true) {
        return Ok(err(StatusCode::BAD_REQUEST, "messages must end with a user turn"));
    }

    let mut msgs: Vec<Value> = vec![json!({"role": "system", "content": system_prompt(&state)})];
    msgs.extend(messages.iter().map(|m| json!({"role": m.role, "content": m.content})));
    let tools = tools();
    let mut tool_results: Vec<ToolResult> = vec![];
    for _round in 0..4 {
        let req = json!({"model": state.cfg.guide_ai_model, "messages": msgs, "tools": tools, "tool_choice": "auto", "max_tokens": 900, "temperature": 0.3, "chat_template_kwargs": {"enable_thinking": false}});
        let res = state.http.post(format!("{}/chat/completions", state.cfg.guide_ai_url)).bearer_auth(&state.cfg.guide_ai_key).json(&req).send().await;
        let res = match res {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("guide model unreachable: {e}");
                return Ok(err(StatusCode::BAD_GATEWAY, "the model is not answering right now"));
            }
        };
        if !res.status().is_success() {
            let st = res.status();
            let t = res.text().await.unwrap_or_default();
            tracing::warn!("guide model {st}: {}", t.chars().take(300).collect::<String>());
            return Ok(err(StatusCode::BAD_GATEWAY, "the model returned an error"));
        }
        let data: Value = res.json().await.unwrap_or(Value::Null);
        let message = data["choices"][0]["message"].clone();
        let calls = message["tool_calls"].as_array().cloned().unwrap_or_default();
        let text = strip_think(message["content"].as_str().unwrap_or(""));
        if calls.is_empty() {
            return Ok(Json(ChatReply { text, tool_results }).into_response());
        }
        msgs.push(json!({"role": "assistant", "content": message["content"].as_str().unwrap_or(""), "tool_calls": calls}));
        for c in &calls {
            let name = c["function"]["name"].as_str().unwrap_or("").to_string();
            let raw = c["function"]["arguments"].as_str().unwrap_or("{}");
            let args: Value = serde_json::from_str(raw).unwrap_or_else(|_| json!({}));
            let (for_model, for_page) = run_tool(&state, &name, &args).await;
            if let Some(p) = for_page {
                tool_results.push(ToolResult { name: name.clone(), args: args.clone(), result: p });
            }
            msgs.push(json!({"role": "tool", "tool_call_id": c["id"], "name": name, "content": for_model.to_string()}));
        }
    }
    Ok(Json(ChatReply { text: "Here is what the catalog returned. Nathaniel can take it from here: (847) 708-5062 or info@thepenmarket.com.".into(), tool_results }).into_response())
}

fn strip_think(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}
