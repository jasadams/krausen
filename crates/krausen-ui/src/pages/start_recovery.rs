// SPDX-License-Identifier: AGPL-3.0-or-later
//! The original Start brew request, retained from the moment it is dispatched
//! until its result is known (INT-004). It is a finite per-request record, not a
//! draft: closing the task, navigating or reloading never discards it, and it is
//! replayed verbatim (same request identity and payload) by an explicit check.
//! The shared start operation is idempotent on `request_id`, so two tabs that
//! both replay one record cannot create two brews. Scope is installation,
//! account and workspace; a record belongs to exactly one recipe.
use krausen_types::{brew::StartBrewRequest, request_context::OperationIntent};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Record {
    version: u8,
    installation: String,
    request: StartBrewRequest,
    /// Creation time (ms since epoch); orders unresolved records oldest first.
    #[serde(default)]
    created_ms: u64,
}

pub(crate) const PREFIX: &str = "krausen-start-v1:";

fn key(installation: &str, request: &StartBrewRequest) -> Result<String, String> {
    serde_json::to_string(&(
        installation,
        &request.intent.account_id,
        &request.intent.workspace_id,
        &request.recipe_id,
        &request.request_id,
    ))
    .map(|tuple| format!("{PREFIX}{tuple}"))
    .map_err(|e| e.to_string())
}

/// (installation, account, workspace, recipe, request) named by a storage key.
fn key_scope(key: &str) -> Option<(String, String, String, String, String)> {
    serde_json::from_str(key.strip_prefix(PREFIX)?).ok()
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Result<web_sys::Storage, String> {
    crate::utils::storage::local_storage("Start recovery")
}

#[cfg(target_arch = "wasm32")]
fn now_ms() -> u64 {
    js_sys::Date::now() as u64
}
#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> u64 {
    0
}

/// Retains the exact request before it is sent. Without durable storage the
/// request cannot be recovered after a reload, so dispatch is refused rather
/// than risking an unrecoverable duplicate. A different payload already owning
/// the identity is refused, never replaced.
pub(crate) fn retain(installation: &str, request: &StartBrewRequest) -> Result<(), String> {
    let key = key(installation, request)?;
    let record = Record {
        version: 1,
        installation: installation.to_owned(),
        request: request.clone(),
        created_ms: now_ms(),
    };
    let json = krausen_types::request_context::checked_json(&record).map_err(|_| {
        "The start request cannot be retained without changing its numbers".to_owned()
    })?;
    #[cfg(target_arch = "wasm32")]
    {
        let storage = storage()?;
        if let Some(existing) = storage
            .get_item(&key)
            .map_err(|_| "Unable to read start recovery")?
        {
            let previous: Record = serde_json::from_str(&existing)
                .map_err(|_| "An earlier start recovery record is unreadable")?;
            return if previous.request == *request {
                Ok(())
            } else {
                Err("A different payload already owns this start request identity".into())
            };
        }
        storage
            .set_item(&key, &json)
            .map_err(|_| "Unable to retain the original start request")?;
    }
    let _ = (key, json);
    Ok(())
}

/// Forgets a request whose result is known (success or a typed rejection).
pub(crate) fn remove(installation: &str, request: &StartBrewRequest) {
    #[cfg(target_arch = "wasm32")]
    if let Ok(key) = key(installation, request) {
        match storage() {
            Ok(storage) => {
                if storage.remove_item(&key).is_err() {
                    tracing::warn!("Unable to remove resolved start recovery");
                }
            }
            Err(error) => tracing::warn!(error, "Unable to remove resolved start recovery"),
        }
    }
    let _ = (installation, request);
}

/// Pure selection over the stored entries (`key`, `value`) so the fail-closed
/// rules are testable without a browser. Only entries whose key names exactly
/// this installation, account, workspace and recipe are considered, so a corrupt
/// record for another recipe never blocks this one. An unreadable or mismatched
/// in-scope record is an error: no new start is permitted until it is resolved.
fn select(
    entries: Vec<(String, Option<String>)>,
    installation: &str,
    intent: &OperationIntent,
    recipe_id: &str,
) -> Result<Vec<StartBrewRequest>, String> {
    let mut found = Vec::new();
    for (stored, value) in entries {
        let Some((i, account, workspace, recipe, _)) = key_scope(&stored) else {
            if stored.starts_with(PREFIX) {
                // The key itself is unreadable, so its recipe cannot be known.
                return Err(
                    "A retained start request cannot be identified; no new start is permitted"
                        .into(),
                );
            }
            continue;
        };
        if i != installation
            || account != intent.account_id
            || workspace != intent.workspace_id
            || recipe != recipe_id
        {
            continue;
        }
        let record: Record = value
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .ok_or("A retained start request is unreadable; no new start is permitted")?;
        if record.version != 1
            || record.installation != installation
            || record.request.intent != *intent
            || record.request.recipe_id != recipe_id
            || key(installation, &record.request).as_deref() != Ok(stored.as_str())
        {
            return Err(
                "A retained start request does not match its scope; no new start is permitted"
                    .into(),
            );
        }
        found.push((record.created_ms, stored, record.request));
    }
    found.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    Ok(found.into_iter().map(|(_, _, request)| request).collect())
}

/// Unresolved requests for this installation/account/workspace/recipe, oldest first.
pub(crate) fn load(
    installation: &str,
    intent: &OperationIntent,
    recipe_id: &str,
) -> Result<Vec<StartBrewRequest>, String> {
    #[cfg(target_arch = "wasm32")]
    {
        let Ok(storage) = storage() else {
            return Ok(Vec::new());
        };
        // Fails closed: a start key whose value cannot be read blocks starting.
        let entries = raw_entries(&storage)?
            .into_iter()
            .map(|(key, value)| {
                value
                    .map(|value| (key, value))
                    .map_err(|()| "Unable to read start recovery".to_owned())
            })
            .collect::<Result<Vec<_>, String>>()?;
        return select(entries, installation, intent, recipe_id);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (installation, intent, recipe_id);
        Ok(Vec::new())
    }
}

/// Every retained start entry (key and raw value; `Err` when the value could not
/// be read). Shared by `load` (which fails closed) and `discard` (which skips).
#[cfg(target_arch = "wasm32")]
fn raw_entries(
    storage: &web_sys::Storage,
) -> Result<Vec<(String, Result<Option<String>, ()>)>, String> {
    let mut raw = Vec::new();
    for index in 0..storage
        .length()
        .map_err(|_| "Unable to list start recovery")?
    {
        let Some(stored) = storage
            .key(index)
            .map_err(|_| "Unable to read start recovery key")?
        else {
            continue;
        };
        if stored.starts_with(PREFIX) {
            let value = storage.get_item(&stored).map_err(|_| ());
            raw.push((stored, value));
        }
    }
    Ok(raw)
}

/// Serialises "check, retain, verify" across tabs of one workspace and recipe
/// with the Web Locks API. The lock is held until [`StartLock::release`] (or
/// drop). Where `navigator.locks` is unavailable this degrades to a no-op and
/// the caller's retain-then-verify re-read is the only protection.
pub(crate) struct StartLock {
    #[cfg(target_arch = "wasm32")]
    release: Option<js_sys::Function>,
}

impl StartLock {
    pub(crate) fn release(self) {}
}

#[cfg(target_arch = "wasm32")]
impl Drop for StartLock {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            if release.call0(&wasm_bindgen::JsValue::NULL).is_err() {
                tracing::warn!("Unable to release the start lock");
            }
        }
    }
}

#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn lock_name(intent: &OperationIntent, recipe_id: &str) -> String {
    format!(
        "krausen-start:{}:{}:{}",
        intent.account_id, intent.workspace_id, recipe_id
    )
}

/// Runs `work` but gives up after `ms`, returning `None`; the abandoned work's
/// result is discarded. Outside the browser there is no timer, so it just runs.
pub(crate) async fn bounded<T: 'static>(
    ms: u32,
    work: impl std::future::Future<Output = T> + 'static,
) -> Option<T> {
    #[cfg(target_arch = "wasm32")]
    {
        use js_sys::{Function, Promise};
        use std::{cell::RefCell, rc::Rc};
        let mut wake = None::<Function>;
        let done = Promise::new(&mut |resolve, _| wake = Some(resolve));
        let Some(wake) = wake else { return None };
        let slot = Rc::new(RefCell::new(None::<T>));
        let finished = slot.clone();
        let wake_on_finish = wake.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let value = work.await;
            *finished.borrow_mut() = Some(value);
            let _ = wake_on_finish.call0(&wasm_bindgen::JsValue::NULL);
        });
        let timeout = gloo_timers::callback::Timeout::new(ms, move || {
            let _ = wake.call0(&wasm_bindgen::JsValue::NULL);
        });
        let _ = wasm_bindgen_futures::JsFuture::from(done).await;
        drop(timeout);
        slot.borrow_mut().take()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = ms;
        Some(work.await)
    }
}

/// Longest wait for another tab's start critical section before degrading.
#[cfg(target_arch = "wasm32")]
const LOCK_WAIT_MS: u32 = 10_000;

pub(crate) async fn acquire_lock(intent: &OperationIntent, recipe_id: &str) -> StartLock {
    #[cfg(target_arch = "wasm32")]
    {
        use js_sys::{Function, Promise, Reflect};
        use wasm_bindgen::{JsCast, JsValue, closure::Closure};
        let none = || StartLock { release: None };
        let Some(window) = web_sys::window() else {
            return none();
        };
        // `web_sys` gates the Web Locks types behind an unstable cfg, so the
        // API is reached dynamically; absence of `navigator.locks` is a fallback.
        let locks = match Reflect::get(&window.navigator(), &JsValue::from_str("locks")) {
            Ok(value) if !value.is_undefined() && !value.is_null() => value,
            _ => return none(),
        };
        let Some(request) = Reflect::get(&locks, &JsValue::from_str("request"))
            .ok()
            .and_then(|value| value.dyn_into::<Function>().ok())
        else {
            return none();
        };
        use std::{cell::Cell, rc::Rc};
        let mut grant = None::<Function>;
        let mut fail = None::<Function>;
        let mut release = None::<Function>;
        let granted = Promise::new(&mut |resolve, reject| {
            grant = Some(resolve);
            fail = Some(reject);
        });
        let hold = Promise::new(&mut |resolve, _| release = Some(resolve));
        let (Some(grant), Some(fail), Some(release)) = (grant, fail, release) else {
            return none();
        };
        // Once the wait is abandoned (rejection or timeout) a late grant must not
        // keep the lock: it is released at once so other tabs are never blocked.
        let abandoned = Rc::new(Cell::new(false));
        let held = hold.clone();
        let late = abandoned.clone();
        let callback = Closure::once(move |_lock: JsValue| -> JsValue {
            if late.get() {
                return Promise::resolve(&JsValue::UNDEFINED).into();
            }
            let _ = grant.call0(&JsValue::NULL);
            held.into()
        });
        let requested = request.call2(
            &locks,
            &JsValue::from_str(&lock_name(intent, recipe_id)),
            callback.as_ref(),
        );
        callback.forget();
        // Failures such as a SecurityError arrive as a rejected promise.
        match requested {
            Ok(promise) => {
                if let Ok(promise) = promise.dyn_into::<Promise>() {
                    let reject = fail.clone();
                    let on_error = Closure::once(move |error: JsValue| {
                        let _ = reject.call1(&JsValue::NULL, &error);
                    });
                    let _ = promise.catch(&on_error);
                    on_error.forget();
                }
            }
            Err(_) => return none(),
        }
        // Bounded wait: another tab's critical section is brief, so a long wait
        // degrades to the retain-then-verify path instead of hanging Start.
        let timeout = {
            let abandoned = abandoned.clone();
            gloo_timers::callback::Timeout::new(LOCK_WAIT_MS, move || {
                abandoned.set(true);
                let _ = fail.call0(&JsValue::NULL);
            })
        };
        let outcome = wasm_bindgen_futures::JsFuture::from(granted).await;
        drop(timeout);
        if outcome.is_err() {
            abandoned.set(true);
            return none();
        }
        StartLock {
            release: Some(release),
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (intent, recipe_id);
        StartLock {}
    }
}

/// Keys an explicit discard may remove. With `request_id` exactly that request of
/// this recipe scope. Without it only the broken entries: unparseable start keys
/// and in-scope entries that are unreadable or do not match their key. A separate
/// readable request is never listed, because a brew may exist for it.
fn discardable(
    entries: &[(String, Option<String>)],
    installation: &str,
    intent: &OperationIntent,
    recipe_id: &str,
    request_id: Option<&str>,
) -> Vec<String> {
    let mut doomed = Vec::new();
    for (stored, value) in entries {
        let Some((i, account, workspace, recipe, request)) = key_scope(stored) else {
            if request_id.is_none() && stored.starts_with(PREFIX) {
                doomed.push(stored.clone());
            }
            continue;
        };
        if i != installation
            || account != intent.account_id
            || workspace != intent.workspace_id
            || recipe != recipe_id
        {
            continue;
        }
        let broken = select(
            vec![(stored.clone(), value.clone())],
            installation,
            intent,
            recipe_id,
        )
        .is_err();
        match request_id {
            Some(id) if id == request => doomed.push(stored.clone()),
            None if broken => doomed.push(stored.clone()),
            _ => {}
        }
    }
    doomed
}

/// A key whose value could not be read is neither listed nor deleted: an error is
/// not evidence that the record is broken.
#[cfg(any(target_arch = "wasm32", test))]
fn readable_entries(
    raw: Vec<(String, Result<Option<String>, ()>)>,
) -> Vec<(String, Option<String>)> {
    raw.into_iter()
        .filter_map(|(key, value)| value.ok().map(|value| (key, value)))
        .collect()
}

/// Explicit user resolution of a retained record. See [`discardable`].
pub(crate) fn discard(
    installation: &str,
    intent: &OperationIntent,
    recipe_id: &str,
    request_id: Option<&str>,
) {
    #[cfg(target_arch = "wasm32")]
    if let Ok(storage) = storage() {
        let Ok(raw) = raw_entries(&storage) else {
            return;
        };
        let entries = readable_entries(raw);
        for stored in discardable(&entries, installation, intent, recipe_id, request_id) {
            if storage.remove_item(&stored).is_err() {
                tracing::warn!("Unable to discard a retained start record");
            }
        }
    }
    let _ = (installation, intent, recipe_id, request_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent() -> OperationIntent {
        OperationIntent {
            account_id: "a".into(),
            workspace_id: "w".into(),
        }
    }
    fn request(recipe: &str, id: &str) -> StartBrewRequest {
        StartBrewRequest {
            intent: intent(),
            recipe_id: recipe.into(),
            selected_recipe_revision: 2,
            expected_head_revision: Some(2),
            expected_calculation_context: "digest".into(),
            request_id: id.into(),
        }
    }
    fn entry(request: &StartBrewRequest, created_ms: u64) -> (String, Option<String>) {
        let json = krausen_types::request_context::checked_json(&Record {
            version: 1,
            installation: "http://local".into(),
            request: request.clone(),
            created_ms,
        })
        .unwrap();
        (key("http://local", request).unwrap(), Some(json))
    }

    #[test]
    fn retained_request_round_trips_with_its_original_identity_and_payload() {
        let original = request("r", "request-1");
        let (stored, json) = entry(&original, 5);
        let recovered: Record = serde_json::from_str(&json.unwrap()).unwrap();
        assert_eq!(recovered.request, original);
        assert_eq!(
            key_scope(&stored),
            Some((
                "http://local".into(),
                "a".into(),
                "w".into(),
                "r".into(),
                "request-1".into()
            ))
        );
        assert_ne!(
            stored,
            key("http://local", &request("r", "request-2")).unwrap()
        );
        assert_ne!(stored, key("http://other", &original).unwrap());
    }

    #[test]
    fn records_are_ordered_oldest_first_not_by_key() {
        let newer = request("r", "aaa");
        let older = request("r", "zzz");
        let found = select(
            vec![entry(&newer, 20), entry(&older, 10)],
            "http://local",
            &intent(),
            "r",
        )
        .unwrap();
        assert_eq!(found, vec![older, newer]);
    }

    #[test]
    fn a_corrupt_record_blocks_only_its_own_recipe() {
        let good = request("other", "g");
        let bad_key = key("http://local", &request("r", "bad")).unwrap();
        let entries = vec![entry(&good, 1), (bad_key, Some("{not json".into()))];
        assert!(select(entries.clone(), "http://local", &intent(), "r").is_err());
        assert_eq!(
            select(entries, "http://local", &intent(), "other").unwrap(),
            vec![good]
        );
    }

    #[test]
    fn a_record_that_does_not_match_its_key_fails_closed() {
        let genuine = request("r", "one");
        let (_, json) = entry(&request("r", "two"), 1);
        let forged = (key("http://local", &genuine).unwrap(), json);
        assert!(select(vec![forged], "http://local", &intent(), "r").is_err());
    }

    #[test]
    fn an_unidentifiable_key_blocks_every_recipe() {
        let entries = vec![(format!("{PREFIX}garbage"), Some("x".into()))];
        assert!(select(entries, "http://local", &intent(), "r").is_err());
    }

    #[test]
    fn other_accounts_and_unrelated_storage_are_ignored() {
        let mut foreign = request("r", "f");
        foreign.intent.account_id = "someone-else".into();
        let entries = vec![entry(&foreign, 1), ("unrelated".into(), Some("x".into()))];
        assert_eq!(
            select(entries, "http://local", &intent(), "r").unwrap(),
            Vec::new()
        );
    }

    #[test]
    fn discarding_a_broken_record_clears_the_block_without_touching_a_readable_request() {
        let readable = request("r", "keep");
        let bad_key = key("http://local", &request("r", "bad")).unwrap();
        let entries = vec![
            entry(&readable, 1),
            (bad_key.clone(), Some("{not json".into())),
            (format!("{PREFIX}garbage"), Some("x".into())),
        ];
        let doomed = discardable(&entries, "http://local", &intent(), "r", None);
        assert!(doomed.contains(&bad_key) && doomed.contains(&format!("{PREFIX}garbage")));
        assert_eq!(doomed.len(), 2);
        let remaining: Vec<_> = entries
            .into_iter()
            .filter(|(k, _)| !doomed.contains(k))
            .collect();
        assert_eq!(
            select(remaining, "http://local", &intent(), "r").unwrap(),
            vec![readable]
        );
    }

    #[test]
    fn discarding_an_identified_request_removes_only_that_request() {
        let one = request("r", "one");
        let two = request("r", "two");
        let other = request("other", "one");
        let entries = vec![entry(&one, 1), entry(&two, 2), entry(&other, 3)];
        let doomed = discardable(&entries, "http://local", &intent(), "r", Some("one"));
        assert_eq!(doomed, vec![key("http://local", &one).unwrap()]);
    }

    #[test]
    fn the_lock_is_scoped_to_account_workspace_and_recipe() {
        let name = lock_name(&intent(), "r");
        assert_eq!(name, "krausen-start:a:w:r");
        assert_ne!(name, lock_name(&intent(), "other"));
    }

    #[test]
    fn an_unreadable_key_is_never_discarded() {
        let good = request("r", "keep");
        let unreadable = key("http://local", &request("r", "unread")).unwrap();
        let (good_key, good_value) = entry(&good, 1);
        let entries = readable_entries(vec![
            (good_key, Ok(good_value)),
            (unreadable.clone(), Err(())),
        ]);
        assert!(!entries.iter().any(|(k, _)| *k == unreadable));
        assert!(discardable(&entries, "http://local", &intent(), "r", None).is_empty());
    }
}
