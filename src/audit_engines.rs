//! Opt-in bounded review modules over paired evidence. No network or mutation.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
pub fn definition() -> Value {
    json!({"type":"function","function":{"name":"review_web_evidence","description":"Select exactly one bounded passive review of an existing paired observation: form_csrf_review, response_policy_review, reflection_review (requires a harmless marker), reflection_controlled_comparison (requires marker, variant_marker, variant_evidence and repeated_control_evidence), redirect_review, or redirect_controlled_comparison (requires variant_evidence and repeated_control_evidence). Requires current explicit read scope. Makes zero requests and never submits forms, follows redirects or confirms exploitability. Returns evidence references, limits and a falsifiable next test; active tests require separate scope and approval.","parameters":{"type":"object","properties":{"engine":{"type":"string","enum":["form_csrf_review","response_policy_review","reflection_review","reflection_controlled_comparison","redirect_review","redirect_controlled_comparison"]},"evidence_id":{"type":"string"},"marker":{"type":"string","minLength":8,"maxLength":64},"variant_marker":{"type":"string","minLength":8,"maxLength":64},"variant_evidence":{"type":"string"},"repeated_control_evidence":{"type":"string"}},"required":["engine","evidence_id"],"additionalProperties":false}}})
}
pub fn review(session: &crate::session::Session, args: &Value) -> Result<Value> {
    let engine = args["engine"].as_str().context("Select one engine")?;
    if !matches!(
        engine,
        "form_csrf_review"
            | "reflection_review"
            | "reflection_controlled_comparison"
            | "response_policy_review"
            | "redirect_review"
            | "redirect_controlled_comparison"
    ) {
        bail!("Unknown review engine");
    }
    if !session.scope.enabled {
        bail!("Explicit user-managed scope is required");
    }
    let id = args["evidence_id"]
        .as_str()
        .context("evidence_id required")?;
    let (tool, input, result) = crate::audit::request_for(session, id)?;
    if crate::reasoning::claims::is_historical(&result)
        || result["denied"] == true
        || result["execution_status"] == "unknown"
        || result["ok"] == false
    {
        bail!("Review requires usable independent evidence");
    }
    let row = match engine {
        "reflection_controlled_comparison" => controlled_reflection(session, args)?,
        "reflection_review" => reflection(session, args, &tool, &input, &result)?,
        "response_policy_review" => response_policies(session, &tool, &input, &result)?,
        "redirect_controlled_comparison" => controlled_redirect(session, args)?,
        "form_csrf_review" => {
            let (page, pointer) = if tool == "browser" && input["action"] == "snapshot" {
                (&result["result"], "/result/forms")
            } else if tool == "browser_url_inspection" {
                (&result["page"], "/page/forms")
            } else {
                bail!("Form review requires a verified native browser snapshot or automatic inspection");
            };
            session.scope.check(
                page["url"].as_str().context("Snapshot URL missing")?,
                "read",
            )?;
            let forms = page["forms"]
                .as_array()
                .context("Form metadata unavailable")?;
            let rows=forms.iter().take(20).enumerate().map(|(index,form)| {
                let authorized=form["action"].as_str().is_some_and(|url|session.scope.check(url,"read").is_ok());
                if !authorized {return json!({"form_index":index,"state":"outside_scope_or_missing_action"});}
                let fields=form["fields"].as_array();
                let (fields_omitted, mut incomplete) = collection_limits(form, "fields", fields.map(Vec::len), 60);
                incomplete |= fields.is_some_and(|items|items.iter().take(60).any(|f|f["name_truncated"]==true || (f["name_truncated"].is_null() && f["name"].as_str().is_some_and(|n|n.len()>=100))));
                let token_like=fields.is_some_and(|fields|fields.iter().take(60).any(|f|f["name"].as_str().is_some_and(|name|{let name=name.to_ascii_lowercase();name.contains("csrf")||name.contains("xsrf")})));
                json!({"form_index":index,"state":"indicator_only","post_method_observed":form["method"].as_str().is_some_and(|m|m.eq_ignore_ascii_case("post")),"token_like_name_observed":token_like,"fields_metadata_available":fields.is_some(),"fields_scanned":fields.map(|f|f.len().min(60)),"fields_omitted":fields_omitted,"fields_metadata_complete":!incomplete,"token_name_assessment":if token_like {"token_like_name_observed"} else if incomplete {"inconclusive_missing_or_bounded_metadata"} else {"not_observed_in_supplied_fields"},"evidence_pointer":format!("{pointer}/{index}"),"protection_verified":false})
            }).collect::<Vec<_>>();
            let (forms_omitted, forms_incomplete) =
                collection_limits(page, "forms", Some(forms.len()), 20);
            json!({"forms":rows,"forms_omitted":forms_omitted,"forms_metadata_complete":!forms_incomplete,"hypothesis":"A state-changing form may require request-origin protection; token-like names alone do not prove protection, and absent names do not prove CSRF.","next_test":"In an authorized fixture, identify actual state change and compare an approved normal request with one changed origin/token condition; inspect server-side effect and repeat controls.","confirms":"Only a reproducible unauthorized state change under realistic browser/session conditions could support the hypothesis.","refutes_for_test":"The variant is rejected and no state change occurs under the tested conditions; this does not prove all paths safe.","limits":"Bounded DOM metadata only. Cookie SameSite, Origin/Referer validation, JavaScript-added tokens and server behavior were not tested. Input values are not inspected or exported."})
        }
        _ => {
            if tool != "web_fetch" {
                bail!("Redirect review requires a paired web_fetch result");
            }
            let target = format!(
                "{}{}",
                input
                    .pointer("/url/origin")
                    .and_then(Value::as_str)
                    .context("Origin missing")?,
                input
                    .pointer("/url/path")
                    .and_then(Value::as_str)
                    .context("Path missing")?
            );
            session.scope.check(&target, "read")?;
            let status = result["status"]
                .as_u64()
                .context("Native HTTP status missing")?;
            let metadata = &result["redirect"];
            json!({"status":status,"redirect_status_observed":(300..400).contains(&status),"location_metadata_available":metadata.is_object(),"cross_origin":metadata["cross_origin"],"followed":false,"hypothesis":"A redirect may depend on untrusted input; an observed redirect alone does not demonstrate an open redirect.","next_test":"In an authorized fixture, compare the normal request with exactly one destination input changed to a harmless controlled origin. Do not follow the destination or send credentials.","confirms":"Reproducible control of the redirect destination beyond the intended allowlist would support the hypothesis; native status alone does not.","refutes_for_test":"The changed destination is rejected or constrained to the intended allowlist in repeated controls.","limits":"Only native status and redacted Location metadata reviewed. No destination fetch, input-control proof, authentication bypass or impact is inferred."})
        }
    };
    Ok(
        json!({"ok":true,"engine":engine,"source_evidence_id":id,"result_sha256":crate::intelligence::code::hash(result.to_string().as_bytes()),"network_requests":0,"state_changes":0,"fresh_evidence":false,"verdict":"hypothesis_review_only","vulnerability_confirmed":false,"review":row}),
    )
}
// Missing old collector counters at its exact cap cannot establish completeness.
fn collection_limits(
    owner: &Value,
    key: &str,
    count: Option<usize>,
    cap: usize,
) -> (Option<usize>, bool) {
    let Some(count) = count else {
        return (None, true);
    };
    let total = owner[format!("{key}_total")]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok());
    let omitted = owner[format!("{key}_omitted")]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok());
    let truncated = owner[format!("{key}_truncated")].as_bool();
    let counters_consistent = total.is_some_and(|n| n >= count && omitted == Some(n - count));
    let known_complete = counters_consistent && total == Some(count) && truncated == Some(false);
    let incomplete = count > cap
        || truncated == Some(true)
        || omitted.is_some_and(|n| n > 0)
        || total.is_some_and(|n| n != count)
        || (count >= cap && !known_complete)
        || ((total.is_some() || omitted.is_some()) && !counters_consistent);
    let removed = if counters_consistent {
        total.map(|n| n.saturating_sub(count.min(cap)))
    } else if incomplete {
        None
    } else {
        Some(0)
    };
    (removed, incomplete)
}
/// Literal marker review only: no HTML parsing, execution or source attribution.
fn reflection(
    session: &crate::session::Session,
    args: &Value,
    tool: &str,
    input: &Value,
    result: &Value,
) -> Result<Value> {
    if tool != "web_fetch" {
        bail!("Reflection review requires a paired web_fetch result");
    }
    let marker = args["marker"]
        .as_str()
        .context("Harmless marker required")?;
    if !(8..=64).contains(&marker.len())
        || !marker
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        bail!(
            "Use an 8-64 byte ASCII alphanumeric/underscore/hyphen marker, not executable syntax"
        );
    }
    let origin = input
        .pointer("/url/origin")
        .and_then(Value::as_str)
        .context("Origin missing")?;
    let path = input
        .pointer("/url/path")
        .and_then(Value::as_str)
        .context("Path missing")?;
    session.scope.check(&format!("{origin}{path}"), "read")?;
    let supplied = input
        .pointer("/url/query")
        .and_then(Value::as_object)
        .is_some_and(|query| {
            query.values().any(|values| {
                values
                    .as_array()
                    .is_some_and(|values| values.iter().any(|v| v.as_str() == Some(marker)))
            })
        });
    if !supplied {
        bail!("Marker must equal a decoded query value in the original paired request");
    }
    let text = result["text"]
        .as_str()
        .context("Native response text unavailable")?;
    let mut end = text.len().min(64000);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let count = text[..end].match_indices(marker).take(101).count();
    let incomplete = result["truncated"] != false || end < text.len();
    let mime = result["content_type"]
        .as_str()
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    Ok(
        json!({"state":if count>0 {"literal_marker_observed"} else if incomplete {"inconclusive_incomplete_body"} else {"marker_not_observed_in_captured_text"},
        "marker_sha256":crate::intelligence::code::hash(marker.as_bytes()),"marker_in_original_query":true,"literal_occurrences":count.min(100),"occurrence_count_capped":count>100,"scanned_bytes":end,"capture_incomplete":incomplete,
        "html_mime_observed":mime.eq_ignore_ascii_case("text/html"),"evidence_pointer":"/text","execution_observed":false,"input_control_proven":false,
        "hypothesis":"Literal marker presence can guide an XSS investigation but may be static content, escaped text or an inert representation. It does not establish reflection causality or execution.",
        "next_test":"In an authorized local fixture, compare a baseline, one changed harmless marker, and a repeated control; then inspect actual rendering context and separately approved browser execution evidence. Do not infer execution from source text.",
        "confirms":"A separately verified browser execution event attributable to controlled input and relevant impact is needed; this engine cannot confirm XSS.",
        "refutes_for_test":"No execution under the tested browser and context limits that test only. Marker absence in captured text does not establish safety.",
        "limits":"Literal case-sensitive matching only, at most 64000 UTF-8 bytes and 100 counted matches. No decoding, HTML/JavaScript parsing, session-cookie context, browser rendering or execution. Raw marker, URL, response excerpts and content-type values are omitted."}),
    )
}
fn controlled_reflection(session: &crate::session::Session, args: &Value) -> Result<Value> {
    let ids = [
        "evidence_id",
        "variant_evidence",
        "repeated_control_evidence",
    ]
    .map(|key| {
        args[key]
            .as_str()
            .context("Three evidence references required")
    });
    let [a, b, c] = ids;
    let ids = [a?, b?, c?];
    if ids[0] == ids[1] || ids[0] == ids[2] || ids[1] == ids[2] {
        bail!("Three distinct observations required");
    }
    let positions = ids.map(|id| {
        session
            .observations
            .iter()
            .find(|o| o.id == id)
            .map(|o| o.message_index)
    });
    if !matches!(positions,[Some(a),Some(b),Some(c)] if a<b && b<c) {
        bail!("Require baseline, variant, repeated control in order");
    }
    let marker = args["marker"]
        .as_str()
        .context("Baseline marker required")?;
    let variant = args["variant_marker"]
        .as_str()
        .context("Variant marker required")?;
    if marker == variant || marker.contains(variant) || variant.contains(marker) {
        bail!("Use distinct non-overlapping harmless markers");
    }
    let pair = crate::audit::compare(
        session,
        &json!({"baseline_evidence":ids[0],"variant_evidence":ids[1]}),
    )?;
    crate::audit::compare(
        session,
        &json!({"baseline_evidence":ids[2],"variant_evidence":ids[1]}),
    )?;
    let mut inputs = Vec::new();
    let mut results = Vec::new();
    let mut summaries = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        let (tool, input, result) = crate::audit::request_for(session, id)?;
        let summary = reflection(
            session,
            &json!({"marker":if index==1 {variant} else {marker}}),
            &tool,
            &input,
            &result,
        )?;
        inputs.push(input);
        results.push(result);
        summaries.push(summary);
    }
    if inputs[0] != inputs[2] {
        bail!("Repeated control must retain the exact normalized baseline input");
    }
    let changed = pair["changed_argument_fields"][0]
        .as_str()
        .context("Changed input missing")?;
    if !changed.starts_with("/url/query/")
        || inputs[0].pointer(changed).and_then(Value::as_str) != Some(marker)
        || inputs[1].pointer(changed).and_then(Value::as_str) != Some(variant)
    {
        bail!(
            "The only changed field must replace the baseline query marker with the variant marker"
        );
    }
    // Restrict the comparative classification to complete successful bodies.
    let usable = results.iter().zip(&summaries).all(|(r, s)| {
        r["status"]
            .as_u64()
            .is_some_and(|n| (200..300).contains(&n))
            && s["capture_incomplete"] == false
            && r["content_type"]
                .as_str()
                .is_some_and(|value| !value.trim().is_empty())
    });
    let stable = results[0]["status"] == results[2]["status"]
        && results[0]["content_type"] == results[2]["content_type"]
        && results[0]["text"] == results[2]["text"];
    let same_response_kind = results[0]["status"] == results[1]["status"]
        && results[0]["content_type"] == results[1]["content_type"];
    let matrix=results.iter().map(|r| {
        let text=r["text"].as_str().unwrap_or("");
        // reflection already checks the size; never scan more than that budget here.
        let mut end=text.len().min(64000); while !text.is_char_boundary(end) {end-=1;}
        json!({"baseline_marker_seen":text[..end].contains(marker),"variant_marker_seen":text[..end].contains(variant)})
    }).collect::<Vec<_>>();
    let differential = matrix[0]["baseline_marker_seen"] == true
        && matrix[0]["variant_marker_seen"] == false
        && matrix[1]["baseline_marker_seen"] == false
        && matrix[1]["variant_marker_seen"] == true
        && matrix[2] == matrix[0];
    let verdict = if !usable {
        "inconclusive_incomplete_or_non_success_response"
    } else if !stable {
        "inconclusive_control_changed"
    } else if !same_response_kind {
        "inconclusive_response_kind_changed"
    } else if !differential {
        "inconclusive_marker_not_differential"
    } else {
        "differential_literal_marker_only"
    };
    Ok(
        json!({"baseline_evidence":ids[0],"variant_evidence":ids[1],"repeated_control_evidence":ids[2],"verdict":verdict,"control_body_and_metadata_consistent":stable,"complete_successful_captures":usable,"same_response_kind":same_response_kind,"marker_observations":matrix,"marker_sha256":crate::intelligence::code::hash(marker.as_bytes()),"variant_marker_sha256":crate::intelligence::code::hash(variant.as_bytes()),"execution_observed":false,"causality_proven":false,
        "hypothesis":"A differential literal marker pattern supports investigating input-dependent output, but does not establish script execution, causal certainty or impact.",
        "next_test":"Inspect rendering context in a separately authorized local browser test; record actual execution evidence and repeat stable controls before assessing impact.",
        "confirms":"Only separately verified execution attributable to controlled input and relevant impact could support XSS; this comparison cannot confirm it.",
        "refutes_for_test":"No execution under the tested context limits that test only; inert or escaped differential output is still not a vulnerability.",
        "limits":"Three saved GET observations, no new requests. Literal markers only; response body/status/type repeat control is not proof of stable account/session/server state. No DOM parsing, JavaScript execution, raw bodies or markers exported."}),
    )
}
fn response_policies(
    session: &crate::session::Session,
    tool: &str,
    input: &Value,
    result: &Value,
) -> Result<Value> {
    // A native GET is one response, not a browser network log or session.
    let native = if tool == "web_fetch" {
        let origin = input
            .pointer("/url/origin")
            .and_then(Value::as_str)
            .context("Origin missing")?;
        let path = input
            .pointer("/url/path")
            .and_then(Value::as_str)
            .context("Path missing")?;
        let target = format!("{origin}{path}");
        session.scope.check(&target, "read")?;
        Some(
            json!({"entries":[{"event":"response","url":target,"security_header_indicators":result["security_header_indicators"]}],"entries_omitted":0,"ring_entries_omitted":0}),
        )
    } else {
        None
    };
    let (entries, pointer) = if let Some(native) = native.as_ref() {
        (&native["entries"], "")
    } else if tool == "browser" && input["action"] == "network" {
        (&result["result"]["entries"], "/result/entries")
    } else if tool == "browser_url_inspection" {
        (&result["network"]["entries"], "/network/entries")
    } else {
        bail!("Response policy review requires verified native HTTP or browser network evidence");
    };
    let entries = entries
        .as_array()
        .context("Network response metadata unavailable")?;
    let mut rows = Vec::new();
    let mut out_of_scope = 0;
    for (index, entry) in entries.iter().take(100).enumerate() {
        if entry["event"] != "response" {
            continue;
        }
        if !entry["url"]
            .as_str()
            .is_some_and(|url| session.scope.check(url, "read").is_ok())
        {
            out_of_scope += 1;
            continue;
        }
        if rows.len() >= 20 {
            break;
        }
        let evidence_pointer = if native.is_some() {
            "/security_header_indicators".to_owned()
        } else {
            format!("{pointer}/{index}")
        };
        if entry["url_truncated"] == true {
            rows.push(
                json!({"evidence_pointer":evidence_pointer.clone(),"state":"metadata_incomplete"}),
            );
            continue;
        }
        if let Some(indicators) = entry["security_header_indicators"].as_object() {
            let keys = [
                "x_frame_options_known_value_seen",
                "csp_header_frame_directive_seen",
                "report_only_frame_directive_seen",
                "nosniff_value_seen",
            ];
            if indicators.get("available") != Some(&json!(true))
                || indicators.get("complete") != Some(&json!(true))
                || keys
                    .iter()
                    .any(|key| !indicators.get(*key).is_some_and(Value::is_boolean))
            {
                rows.push(json!({"evidence_pointer":evidence_pointer.clone(),"state":"header_metadata_incomplete_or_unavailable","missing_protection_inferred":false}));
            } else {
                let mut row = json!({"evidence_pointer":evidence_pointer.clone(),"state":"indicators_only","protection_verified":false,"collection":"native_boolean_summary"});
                for key in keys {
                    row[key] = indicators[key].clone();
                }
                rows.push(row);
            }
            continue;
        }
        let Some(headers) = entry["security_headers"].as_object() else {
            rows.push(json!({"evidence_pointer":evidence_pointer.clone(),"state":"header_metadata_unavailable","missing_protection_inferred":false}));
            continue;
        };
        let header = |name: &str| headers.get(name).and_then(Value::as_str).unwrap_or("");
        let has_frame = |value: &str| {
            value.split(';').any(|directive| {
                directive
                    .split_whitespace()
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case("frame-ancestors"))
            })
        };
        let xframe = header("x-frame-options").trim();
        rows.push(json!({"evidence_pointer":evidence_pointer.clone(),"state":"indicators_only","x_frame_options_known_value_seen":xframe.eq_ignore_ascii_case("deny")||xframe.eq_ignore_ascii_case("sameorigin"),
            "csp_header_frame_directive_seen":has_frame(header("content-security-policy")),"report_only_frame_directive_seen":has_frame(header("content-security-policy-report-only")),
            "nosniff_value_seen":header("x-content-type-options").trim().eq_ignore_ascii_case("nosniff"),"protection_verified":false}));
    }
    let envelope = if let Some(native) = native.as_ref() {
        native
    } else if tool == "browser" {
        &result["result"]
    } else {
        &result["network"]
    };
    let counts = [
        envelope["entries_omitted"].as_u64(),
        envelope["ring_entries_omitted"].as_u64(),
    ];
    let capture_partial = if counts.iter().flatten().any(|n| *n > 0) {
        Some(true)
    } else if counts.iter().all(Option::is_some) {
        Some(false)
    } else {
        None
    };
    Ok(
        json!({"collection_context":if native.is_some(){"single_native_GET_without_browser_session"}else{"browser_network_metadata"},"responses":rows,"upstream_capture_partial":capture_partial,"upstream_capture_completeness_known":capture_partial.is_some(),"out_of_scope_entries_skipped":out_of_scope,"entry_scan_limit":100,"response_limit":20,"input_entries":entries.len(),"scan_truncated":entries.len()>100 || rows.len()>=20,
        "hypothesis":"Response policy indicators may warrant a controlled rendering check; absent or report-only metadata does not prove an exploitable weakness.",
        "next_test":"In an authorized local browser fixture, inspect effective rendering and policy enforcement for the exact response, accounting for CSP syntax, redirects and embedding origin. Any active navigation requires separate approval.",
        "confirms":"A reproducible undesired browser behavior and relevant impact under controlled conditions could support a finding; header metadata alone cannot.",
        "refutes_for_test":"The browser enforces the intended policy in the tested configuration; other paths remain untested.",
        "limits":"Allowlisted, possibly truncated header metadata only. Full CSP parsing, browser enforcement, framing impact and content-type execution were not tested. Missing metadata is not missing protection. Raw URLs/header values are omitted."}),
    )
}
fn controlled_redirect(session: &crate::session::Session, args: &Value) -> Result<Value> {
    let baseline = args["evidence_id"]
        .as_str()
        .context("Baseline evidence required")?;
    let variant = args["variant_evidence"]
        .as_str()
        .context("Variant evidence required")?;
    let repeat = args["repeated_control_evidence"]
        .as_str()
        .context("Repeated control evidence required")?;
    if baseline == repeat || variant == repeat || baseline == variant {
        bail!("Three distinct observations are required");
    }
    let positions = [baseline, variant, repeat].map(|id| {
        session
            .observations
            .iter()
            .find(|o| o.id == id)
            .map(|o| o.message_index)
    });
    if !matches!(positions,[Some(a),Some(b),Some(c)] if a<b && b<c) {
        bail!("Require baseline, variant, repeated control in chronological transcript order");
    }
    let pair = crate::audit::compare(
        session,
        &json!({"baseline_evidence":baseline,"variant_evidence":variant}),
    )?;
    crate::audit::compare(
        session,
        &json!({"baseline_evidence":repeat,"variant_evidence":variant}),
    )?;
    let mut inputs = Vec::new();
    let mut metadata = Vec::new();
    for (index, id) in [baseline, variant, repeat].into_iter().enumerate() {
        let (tool, input, result) = crate::audit::request_for(session, id)?;
        if tool != "web_fetch" {
            bail!("Controlled redirect review requires native web_fetch observations");
        }
        if result["execution_status"] == "not_executed" {
            bail!("A non-executed action is not an HTTP observation");
        }
        let target = format!(
            "{}{}",
            input
                .pointer("/url/origin")
                .and_then(Value::as_str)
                .context("Origin missing")?,
            input
                .pointer("/url/path")
                .and_then(Value::as_str)
                .context("Path missing")?
        );
        session.scope.check(&target, "read")?;
        let status = result["status"].as_u64().context("Native status missing")?;
        if !(300..400).contains(&status) {
            if index != 1 || !(200..600).contains(&status) {
                bail!("Normal and repeated controls require redirects; variant requires a native HTTP response");
            }
            metadata.push(json!({"status":status}));
            inputs.push(input);
            continue;
        }
        let location = result
            .pointer("/redirect/location_redacted")
            .and_then(Value::as_str)
            .context("Redacted native Location unavailable")?;
        let location = reqwest::Url::parse(location)?;
        if !matches!(location.scheme(), "http" | "https") {
            bail!("Unsupported redirect location scheme");
        }
        metadata.push(json!({"status":status,"origin":location.origin().ascii_serialization(),"path":location.path()}));
        inputs.push(input);
    }
    if inputs[0] != inputs[2] {
        bail!("Repeated control must use the same normalized input as baseline");
    }
    let stable = metadata[0] == metadata[2];
    let variant_status = metadata[1]["status"].as_u64().unwrap();
    let variant_redirect = (300..400).contains(&variant_status);
    let rejection_status = matches!(variant_status, 400 | 403 | 422);
    let verdict = if !stable {
        "inconclusive_control_changed"
    } else if variant_redirect {
        "controlled_metadata_comparison_only"
    } else if rejection_status {
        "variant_http_rejection_observed"
    } else {
        "inconclusive_variant_non_redirect_response"
    };
    Ok(
        json!({"baseline_evidence":baseline,"variant_evidence":variant,"repeated_control_evidence":repeat,
        "changed_argument_fields":pair["changed_argument_fields"],"control_metadata_consistent":stable,
        "destination_metadata_changed":if variant_redirect {Some(metadata[0]!=metadata[1])} else {None},
        "variant_http_status":variant_status,"variant_redirect_observed":variant_redirect,"protection_verified":false,
        "verdict":verdict,
        "causality_proven":false,"hypothesis":"One changed input may influence the destination; metadata differences alone do not establish an unsafe redirect.",
        "next_test":if stable {"Within authorized conditions, verify input control and intended destination allowlist, repeat controls, and assess impact without following external destinations."} else {"Re-establish stable account/session and server conditions before attributing any difference to the variant."},
        "confirms":"Reproducible destination control beyond the intended allowlist could support the hypothesis after review; this engine does not certify it.",
        "refutes_for_test":"Approved variants remain constrained to the intended destinations under repeated controls.",
        "limits":"Only status, destination origin/path and normalized tool arguments compared. Variant HTTP 400/403/422 is an observed rejection, not proof of intended policy, authorization, protection or absence of a redirect elsewhere; other non-redirect statuses are inconclusive. Destination query/fragment, cookies, timing, account identity and server state are not verified. No destinations followed, no active requests sent."}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(tool: &str, input: Value, result: Value) -> crate::session::Session {
        let root = tempfile::tempdir().unwrap();
        let mut s = crate::session::new(root.path(), "test");
        s.scope.command("allow https://example.test/ read").unwrap();
        s.messages.push(json!({"role":"assistant","tool_calls":[{"id":"c","function":{"name":tool,"arguments":input.to_string()}}]}));
        s.observations
            .push(crate::reasoning::observe(1, tool, &result));
        s.messages
            .push(json!({"role":"tool","tool_call_id":"c","content":result.to_string()}));
        s
    }
    fn marker_comparison_fixture(
        bodies: [&str; 3],
        truncated: bool,
    ) -> (crate::session::Session, Value) {
        let mut s = fixture(
            "web_fetch",
            json!({"url":"https://example.test/?q=BASE_MARKER"}),
            json!({"ok":true,"status":200,"content_type":"text/html","text":bodies[0],"truncated":truncated}),
        );
        for (index, marker) in [(1, "VARIANT_MARKER"), (2, "BASE_MARKER")] {
            let id = format!("c{index}");
            s.messages.push(json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":"web_fetch","arguments":json!({"url":format!("https://example.test/?q={marker}")}).to_string()}}]}));
            let result = json!({"ok":true,"status":200,"content_type":"text/html","text":bodies[index],"truncated":truncated});
            s.observations.push(crate::reasoning::observe(
                s.messages.len(),
                "web_fetch",
                &result,
            ));
            s.messages
                .push(json!({"role":"tool","tool_call_id":id,"content":result.to_string()}));
        }
        (
            s,
            json!({"engine":"reflection_controlled_comparison","evidence_id":"E000001","variant_evidence":"E000003","repeated_control_evidence":"E000005","marker":"BASE_MARKER","variant_marker":"VARIANT_MARKER"}),
        )
    }
    #[test]
    fn marker_comparison_discriminates_static_inert_and_unstable_output() {
        for (bodies, truncated, verdict) in [
            (
                [
                    "&lt;BASE_MARKER&gt;",
                    "&lt;VARIANT_MARKER&gt;",
                    "&lt;BASE_MARKER&gt;",
                ],
                false,
                "differential_literal_marker_only",
            ),
            (
                ["BASE_MARKER VARIANT_MARKER"; 3],
                false,
                "inconclusive_marker_not_differential",
            ),
            (
                ["BASE_MARKER", "VARIANT_MARKER", "changed BASE_MARKER"],
                false,
                "inconclusive_control_changed",
            ),
            (
                ["BASE_MARKER", "VARIANT_MARKER", "BASE_MARKER"],
                true,
                "inconclusive_incomplete_or_non_success_response",
            ),
        ] {
            let (s, args) = marker_comparison_fixture(bodies, truncated);
            let out = review(&s, &args).unwrap();
            assert_eq!(out["review"]["verdict"], verdict);
            assert_eq!(out["vulnerability_confirmed"], false);
            assert_eq!(out["review"]["causality_proven"], false);
            assert_eq!(out["review"]["execution_observed"], false);
            assert!(!out.to_string().contains("BASE_MARKER"));
        }
    }
    #[test]
    fn marker_comparison_does_not_attribute_response_kind_changes() {
        for (field, value, verdict) in [
            ("status", json!(201), "inconclusive_response_kind_changed"),
            (
                "status",
                json!(302),
                "inconclusive_incomplete_or_non_success_response",
            ),
            (
                "content_type",
                json!("application/json"),
                "inconclusive_response_kind_changed",
            ),
            (
                "content_type",
                Value::Null,
                "inconclusive_incomplete_or_non_success_response",
            ),
        ] {
            let (mut s, args) =
                marker_comparison_fixture(["BASE_MARKER", "VARIANT_MARKER", "BASE_MARKER"], false);
            let mut result: Value =
                serde_json::from_str(s.messages[3]["content"].as_str().unwrap()).unwrap();
            result[field] = value;
            s.messages[3]["content"] = result.to_string().into();
            s.observations[1] = crate::reasoning::observe(3, "web_fetch", &result);
            assert_eq!(review(&s, &args).unwrap()["review"]["verdict"], verdict);
        }
    }
    #[test]
    fn marker_comparison_rejects_wrong_changed_factor_duplicate_and_stale() {
        let (mut s, args) =
            marker_comparison_fixture(["BASE_MARKER", "VARIANT_MARKER", "BASE_MARKER"], false);
        let mut duplicate = args.clone();
        duplicate["repeated_control_evidence"] = json!("E000001");
        assert!(review(&s, &duplicate).is_err());
        // Both markers are in every input; only an unrelated factor differs.
        for (index, mode) in [(0, "normal"), (2, "variant"), (4, "normal")] {
            s.messages[index]["tool_calls"][0]["function"]["arguments"]=json!({"url":format!("https://example.test/?a=BASE_MARKER&b=VARIANT_MARKER&mode={mode}")}).to_string().into();
        }
        assert!(review(&s, &args).is_err());
        let (mut s, args) =
            marker_comparison_fixture(["BASE_MARKER", "VARIANT_MARKER", "BASE_MARKER"], false);
        let mut result: Value =
            serde_json::from_str(s.messages[5]["content"].as_str().unwrap()).unwrap();
        result["fresh_evidence"] = json!(false);
        s.messages[5]["content"] = result.to_string().into();
        s.observations[2] = crate::reasoning::observe(5, "web_fetch", &result);
        assert!(review(&s, &args).is_err());
    }
    #[test]
    fn actual_collector_fixture_preserves_omissions_through_review() {
        let page: Value =
            serde_json::from_str(include_str!("../scripts/fixtures/browser-form-limits.json"))
                .unwrap();
        let s = fixture(
            "browser",
            json!({"action":"snapshot"}),
            json!({"ok":true,"result":page}),
        );
        let out = review(
            &s,
            &json!({"engine":"form_csrf_review","evidence_id":"E000001"}),
        )
        .unwrap();
        assert_eq!(out["review"]["forms_omitted"], 1);
        assert_eq!(out["review"]["forms_metadata_complete"], false);
        let form = &out["review"]["forms"][0];
        assert_eq!(form["fields_omitted"], 1);
        assert_eq!(form["fields_metadata_complete"], false);
        assert_eq!(
            form["token_name_assessment"],
            "inconclusive_missing_or_bounded_metadata"
        );
        assert_eq!(out["vulnerability_confirmed"], false);
    }
    #[test]
    fn legacy_exact_cap_and_inconsistent_counts_are_incomplete() {
        for (value, incomplete, omitted) in [
            (json!({}), true, None),
            (
                json!({"fields_total":60,"fields_omitted":0,"fields_truncated":false}),
                false,
                Some(0),
            ),
            (
                json!({"fields_total":61,"fields_omitted":1,"fields_truncated":true}),
                true,
                Some(1),
            ),
            (
                json!({"fields_total":59,"fields_omitted":0,"fields_truncated":false}),
                true,
                None,
            ),
            (json!({"fields_truncated":true}), true, None),
        ] {
            assert_eq!(
                collection_limits(&value, "fields", Some(60), 60),
                (omitted, incomplete)
            );
        }
        assert_eq!(
            collection_limits(&json!({}), "fields", None, 60),
            (None, true)
        );
        assert_eq!(
            collection_limits(&json!({}), "forms", Some(20), 20),
            (None, true)
        );
    }
    #[test]
    fn form_review_exposes_field_limits_without_inferring_absent_protection() {
        let mut fields = vec![json!({"name":"ordinary"}); 60];
        fields.push(json!({"name":"csrf_token", "value":"PRIVATE_TOKEN"}));
        let s = fixture(
            "browser",
            json!({"action":"snapshot"}),
            json!({"ok":true,"result":{"url":"https://example.test/","forms":[
                {"action":"https://example.test/submit","method":"post","fields":fields,"fields_total":61,"fields_omitted":0,"fields_truncated":false},
                {"action":"https://example.test/submit","method":"post"},
                {"action":"https://example.test/submit","method":"post","fields":[]}
            ]}}),
        );
        let out = review(
            &s,
            &json!({"engine":"form_csrf_review","evidence_id":"E000001"}),
        )
        .unwrap();
        let forms = &out["review"]["forms"];
        assert_eq!(forms[0]["fields_scanned"], 60);
        assert_eq!(forms[0]["fields_omitted"], 1);
        assert_eq!(
            forms[0]["token_name_assessment"],
            "inconclusive_missing_or_bounded_metadata"
        );
        assert_eq!(
            forms[1]["token_name_assessment"],
            "inconclusive_missing_or_bounded_metadata"
        );
        assert!(forms[1]["fields_scanned"].is_null());
        assert_eq!(
            forms[2]["token_name_assessment"],
            "not_observed_in_supplied_fields"
        );
        assert_eq!(out["vulnerability_confirmed"], false);
        assert!(!out.to_string().contains("PRIVATE_TOKEN"));
    }
    #[test]
    fn reflection_indicators_never_claim_execution_or_causality() {
        for (text, mime) in [
            ("<p>BD_MARKER_123</p>", "text/html"),
            ("&lt;script&gt;BD_MARKER_123&lt;/script&gt;", "text/html"),
            ("{\"value\":\"BD_MARKER_123\"}", "application/json"),
            ("static documentation BD_MARKER_123", "text/plain"),
        ] {
            let s = fixture(
                "web_fetch",
                json!({"url":"https://example.test/?q=BD_MARKER_123"}),
                json!({"ok":true,"status":200,"text":text,"content_type":mime,"truncated":false}),
            );
            let out = review(&s, &json!({"engine":"reflection_review","evidence_id":"E000001","marker":"BD_MARKER_123"})).unwrap();
            assert_eq!(out["review"]["state"], "literal_marker_observed");
            assert_eq!(out["review"]["execution_observed"], false);
            assert_eq!(out["review"]["input_control_proven"], false);
            assert_eq!(out["vulnerability_confirmed"], false);
            assert!(!out.to_string().contains("BD_MARKER_123"));
        }
    }
    #[test]
    fn reflection_missing_truncated_or_unrelated_inputs_remain_inconclusive() {
        let args =
            json!({"engine":"reflection_review","evidence_id":"E000001","marker":"BD_MARKER_123"});
        for truncated in [json!(true), json!(null), json!(false)] {
            let s = fixture(
                "web_fetch",
                json!({"url":"https://example.test/?q=BD_MARKER_123"}),
                json!({"ok":true,"text":"absent","truncated":truncated}),
            );
            let out = review(&s, &args).unwrap();
            assert_eq!(
                out["review"]["state"],
                if truncated == false {
                    "marker_not_observed_in_captured_text"
                } else {
                    "inconclusive_incomplete_body"
                }
            );
        }
        let s = fixture(
            "web_fetch",
            json!({"url":"https://example.test/?q=other"}),
            json!({"ok":true,"text":"BD_MARKER_123","truncated":false}),
        );
        assert!(review(&s, &args).is_err());
        let mut invalid = args;
        invalid["marker"] = json!("<script>");
        assert!(review(&s, &invalid).is_err());
    }
    #[test]
    fn reflection_scans_are_bounded_and_require_body_metadata() {
        let args =
            json!({"engine":"reflection_review","evidence_id":"E000001","marker":"BD_MARKER_123"});
        for result in [json!({"ok":true}), json!({"ok":true,"text":null})] {
            let s = fixture(
                "web_fetch",
                json!({"url":"https://example.test/?q=BD_MARKER_123"}),
                result,
            );
            assert!(review(&s, &args).is_err());
        }
        let text = format!("{}BD_MARKER_123", "é".repeat(32001));
        let s = fixture(
            "web_fetch",
            json!({"url":"https://example.test/?q=BD_MARKER_123"}),
            json!({"ok":true,"text":text,"truncated":false}),
        );
        let out = review(&s, &args).unwrap();
        assert_eq!(out["review"]["state"], "inconclusive_incomplete_body");
        assert_eq!(out["review"]["scanned_bytes"], 64000);
        let s = fixture(
            "web_fetch",
            json!({"url":"https://example.test/?q=BD_MARKER_123"}),
            json!({"ok":true,"text":"BD_MARKER_123".repeat(105),"truncated":false}),
        );
        let out = review(&s, &args).unwrap();
        assert_eq!(out["review"]["literal_occurrences"], 100);
        assert_eq!(out["review"]["occurrence_count_capped"], true);
    }
    #[test]
    fn redirect_comparison_requires_repeat_control_and_exposes_instability() {
        let mut s = fixture(
            "web_fetch",
            json!({"url":"https://example.test/redirect?next=normal"}),
            json!({"ok":true,"status":302,"redirect":{"location_redacted":"https://example.test/home"}}),
        );
        for (i, input, location) in [
            (1, "variant", "https://controlled.test/target"),
            (2, "normal", "https://example.test/home"),
        ] {
            let call = format!("c{i}");
            s.messages.push(json!({"role":"assistant","tool_calls":[{"id":call,"function":{"name":"web_fetch","arguments":json!({"url":format!("https://example.test/redirect?next={input}")}).to_string()}}]}));
            let result = json!({"ok":true,"status":302,"redirect":{"location_redacted":location}});
            s.observations.push(crate::reasoning::observe(
                s.messages.len(),
                "web_fetch",
                &result,
            ));
            s.messages
                .push(json!({"role":"tool","tool_call_id":call,"content":result.to_string()}));
        }
        let args = json!({"engine":"redirect_controlled_comparison","evidence_id":"E000001","variant_evidence":"E000003","repeated_control_evidence":"E000005"});
        let result = review(&s, &args).unwrap();
        let mut reversed = args.clone();
        reversed["evidence_id"] = json!("E000005");
        reversed["repeated_control_evidence"] = json!("E000001");
        assert!(review(&s, &reversed).is_err());
        assert_eq!(result["review"]["control_metadata_consistent"], true);
        assert_eq!(result["review"]["destination_metadata_changed"], true);
        assert_eq!(result["vulnerability_confirmed"], false);
        s.messages[5]["content"]=json!({"ok":true,"status":302,"redirect":{"location_redacted":"https://example.test/login"}}).to_string().into();
        // Model a separately observed response, with its own matching digest.
        s.observations[2] = crate::reasoning::observe(
            5,
            "web_fetch",
            &serde_json::from_str::<Value>(s.messages[5]["content"].as_str().unwrap()).unwrap(),
        );
        assert_eq!(
            review(&s, &args).unwrap()["review"]["verdict"],
            "inconclusive_control_changed"
        );
        s.messages[5]["content"]=json!({"ok":true,"status":302,"reused_local_copy":true,"redirect":{"location_redacted":"https://example.test/home"}}).to_string().into();
        s.observations[2].result_sha256 = None; // Legacy freshness rejection.
        assert!(review(&s, &args).is_err());
        let mut duplicate = args.clone();
        duplicate["repeated_control_evidence"] = json!("E000001");
        assert!(review(&s, &duplicate).is_err());
    }
    #[test]
    fn redirect_negative_observations_do_not_prove_protection() {
        for (status, expected) in [
            (400, "variant_http_rejection_observed"),
            (403, "variant_http_rejection_observed"),
            (422, "variant_http_rejection_observed"),
            (200, "inconclusive_variant_non_redirect_response"),
            (401, "inconclusive_variant_non_redirect_response"),
            (429, "inconclusive_variant_non_redirect_response"),
            (503, "inconclusive_variant_non_redirect_response"),
        ] {
            let mut s = fixture(
                "web_fetch",
                json!({"url":"https://example.test/redirect?next=normal"}),
                json!({"ok":true,"status":302,"redirect":{"location_redacted":"https://example.test/home"}}),
            );
            for (index, input, result) in [
                (3, "variant", json!({"ok":status<400,"status":status})),
                (
                    5,
                    "normal",
                    json!({"ok":true,"status":302,"redirect":{"location_redacted":"https://example.test/home"}}),
                ),
            ] {
                let call = format!("c{index}");
                s.messages.push(json!({"role":"assistant","tool_calls":[{"id":call,"function":{"name":"web_fetch","arguments":json!({"url":format!("https://example.test/redirect?next={input}")}).to_string()}}]}));
                s.observations
                    .push(crate::reasoning::observe(index, "web_fetch", &result));
                s.messages
                    .push(json!({"role":"tool","tool_call_id":call,"content":result.to_string()}));
            }
            let args = json!({"engine":"redirect_controlled_comparison","evidence_id":"E000001","variant_evidence":"E000003","repeated_control_evidence":"E000005"});
            let output = review(&s, &args).unwrap();
            assert_eq!(output["review"]["verdict"], expected);
            assert!(output["review"]["destination_metadata_changed"].is_null());
            assert_eq!(output["review"]["protection_verified"], false);
            assert_eq!(output["vulnerability_confirmed"], false);
            for flag in [
                json!({"denied":true}),
                json!({"fresh_evidence":false}),
                json!({"execution_status":"unknown"}),
                json!({"execution_status":"not_executed"}),
            ] {
                let mut blocked: crate::session::Session =
                    serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
                let mut response = json!({"ok":false,"status":status});
                response
                    .as_object_mut()
                    .unwrap()
                    .extend(flag.as_object().unwrap().clone());
                blocked.messages[3]["content"] = json!(response.to_string());
                blocked.observations[1] = crate::reasoning::observe(3, "web_fetch", &response);
                assert!(review(&blocked, &args).is_err());
            }
        }
    }
    #[test]
    fn extension_policy_fixture_is_consumed_without_raw_header_values() {
        let network: Value = serde_json::from_str(include_str!(
            "../scripts/fixtures/browser-policy-network.json"
        ))
        .unwrap();
        let s = fixture(
            "browser",
            json!({"action":"network"}),
            json!({"ok":true,"result":network}),
        );
        let result = review(
            &s,
            &json!({"engine":"response_policy_review","evidence_id":"E000001"}),
        )
        .unwrap();
        let rows = result["review"]["responses"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["collection"], "native_boolean_summary");
        assert_eq!(rows[0]["csp_header_frame_directive_seen"], true);
        assert_eq!(rows[0]["report_only_frame_directive_seen"], true);
        assert_eq!(
            rows[1]["state"],
            "header_metadata_incomplete_or_unavailable"
        );
        assert_eq!(
            rows[2]["state"],
            "header_metadata_incomplete_or_unavailable"
        );
        assert_eq!(result["vulnerability_confirmed"], false);
        assert!(!result.to_string().contains("PRIVATE_"));
    }
    #[test]
    fn native_http_policy_metadata_is_scoped_and_missing_is_unknown() {
        for headers in [
            json!(null),
            json!({"available":true,"complete":false}),
            json!({"available":true,"complete":true,"x_frame_options_known_value_seen":false,"csp_header_frame_directive_seen":false,"report_only_frame_directive_seen":true,"nosniff_value_seen":true}),
        ] {
            let s = fixture(
                "web_fetch",
                json!({"url":"https://example.test/?token=PRIVATE"}),
                json!({"ok":true,"status":200,"security_header_indicators":headers}),
            );
            let output = review(
                &s,
                &json!({"engine":"response_policy_review","evidence_id":"E000001"}),
            )
            .unwrap();
            assert_eq!(
                output["review"]["collection_context"],
                "single_native_GET_without_browser_session"
            );
            assert_eq!(
                output["review"]["responses"][0]["evidence_pointer"],
                "/security_header_indicators"
            );
            assert_eq!(output["vulnerability_confirmed"], false);
            assert_eq!(output["network_requests"], 0);
            let row = &output["review"]["responses"][0];
            if headers["complete"] == true {
                assert_eq!(row["report_only_frame_directive_seen"], true);
                assert_eq!(row["protection_verified"], false);
            } else {
                assert_eq!(row["missing_protection_inferred"], false);
            }
            assert!(!output.to_string().contains("PRIVATE"));
        }
        let s = fixture(
            "web_fetch",
            json!({"url":"https://outside.test/"}),
            json!({"ok":true,"status":200}),
        );
        assert!(review(
            &s,
            &json!({"engine":"response_policy_review","evidence_id":"E000001"})
        )
        .is_err());
    }
    #[test]
    fn response_policy_review_distinguishes_report_only_and_missing_metadata() {
        let s = fixture(
            "browser",
            json!({"action":"network"}),
            json!({"ok":true,"result":{"entries":[
                {"event":"response","url":"https://example.test/?token=SECRET","security_headers":{"content-security-policy-report-only":"frame-ancestors 'none'; report-uri /PRIVATE","x-content-type-options":"nosniff"}},
                {"event":"response","url":"https://example.test/other"},
                {"event":"response","url":"https://outside.test/private","security_headers":{"x-frame-options":"DENY"}},
                {"event":"response","url":"https://example.test/third","security_headers":{"content-security-policy":"default-src 'self'; frame-ancestors 'self'","x-frame-options":"SAMEORIGIN"}}
            ]}}),
        );
        let output = review(
            &s,
            &json!({"engine":"response_policy_review","evidence_id":"E000001"}),
        )
        .unwrap();
        assert!(output["review"]["upstream_capture_partial"].is_null());
        assert_eq!(
            output["review"]["upstream_capture_completeness_known"],
            false
        );
        let rows = output["review"]["responses"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["csp_header_frame_directive_seen"], false);
        assert_eq!(rows[0]["report_only_frame_directive_seen"], true);
        assert_eq!(rows[1]["state"], "header_metadata_unavailable");
        assert_eq!(rows[2]["csp_header_frame_directive_seen"], true);
        assert_eq!(output["review"]["out_of_scope_entries_skipped"], 1);
        assert_eq!(output["vulnerability_confirmed"], false);
        assert!(!output.to_string().contains("SECRET"));
        assert!(!output.to_string().contains("PRIVATE"));
    }
    #[test]
    fn forms_are_only_indicators_and_outside_actions_are_not_reviewed() {
        let s = fixture(
            "browser",
            json!({"action":"snapshot"}),
            json!({"ok":true,"result":{"url":"https://example.test/","forms":[{"action":"https://example.test/save","method":"post","fields":[{"name":"csrf_secret_name","value":"SECRET"}]},{"action":"https://outside.test/save","method":"post"}]}}),
        );
        let result = review(
            &s,
            &json!({"engine":"form_csrf_review","evidence_id":"E000001"}),
        )
        .unwrap();
        assert_eq!(
            result["review"]["forms"][0]["token_like_name_observed"],
            true
        );
        assert_eq!(
            result["review"]["forms"][1]["state"],
            "outside_scope_or_missing_action"
        );
        assert_eq!(result["vulnerability_confirmed"], false);
        assert!(!result.to_string().contains("SECRET"));
        assert!(!result.to_string().contains("csrf_secret_name"));
    }
    #[test]
    fn redirect_status_is_not_proof_and_scope_staleness_are_enforced() {
        let mut s = fixture(
            "web_fetch",
            json!({"url":"https://example.test/?next=SECRET"}),
            json!({"ok":true,"status":302,"redirect":{"cross_origin":true,"followed":false}}),
        );
        let args = json!({"engine":"redirect_review","evidence_id":"E000001"});
        let result = review(&s, &args).unwrap();
        assert_eq!(result["network_requests"], 0);
        assert_eq!(result["vulnerability_confirmed"], false);
        assert!(!result.to_string().contains("SECRET"));
        s.scope.command("clear").unwrap();
        assert!(review(&s, &args).is_err());
        s.scope.command("allow https://example.test/ read").unwrap();
        s.messages[1]["content"] = json!({"ok":true,"status":302,"original_evidence_id":"old"})
            .to_string()
            .into();
        assert!(review(&s, &args).is_err());
    }
}
