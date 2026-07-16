//! Binding value types for the keymap subsystem.
//!
//! Faithful port of the `BindingsMap` half of Python Textual's
//! `textual/binding.py`: an ordered `key -> Vec<BindingDecl>` map constructed
//! from declarative [`BindingDecl`]s, plus the [`Keymap`] overlay
//! (`apply_keymap`) that substitutes keys for bindings addressed by
//! [`BindingDecl::id`].
//!
//! The dispatch path builds a `BindingsMap` on demand from
//! `Widget::bindings()` output (mirroring Python applying the keymap to
//! throwaway copies per chain build in `Screen._binding_chain`); nodes keep
//! declaring plain `Vec<BindingDecl>`.

use std::collections::BTreeMap;

use crate::widgets::BindingDecl;

/// A mapping of binding IDs to key strings, used for overriding default key
/// bindings (Python `Keymap = Mapping[BindingIDString, KeyString]`).
///
/// `BTreeMap` for deterministic iteration (stable clash reporting and
/// reproducible tests). Values are comma-separated key lists, e.g. `"right,k"`.
pub type Keymap = BTreeMap<String, String>;

/// A binding was not found for a key (Python `NoBinding`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoBinding {
    /// The key that had no binding.
    pub key: String,
}

impl std::fmt::Display for NoBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "No binding for {}", self.key)
    }
}

impl std::error::Error for NoBinding {}

/// A binding key is in an invalid format (Python `InvalidBinding`), e.g. an
/// empty alternative in a comma list (`",,,"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidBinding(pub String);

impl std::fmt::Display for InvalidBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for InvalidBinding {}

/// The result of applying a keymap (Python `KeymapApplyResult`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeymapApplyResult {
    /// Bindings that were clashed and replaced by the keymap.
    ///
    /// Python uses a `set[Binding]`; here the list is deduplicated and sorted
    /// by `(key, id)` for deterministic assertions.
    pub clashed_bindings: Vec<BindingDecl>,
}

/// Expand one declarative binding into per-key entries.
///
/// Ports `Binding.make_bindings` (binding.py): comma-list expansion, trim,
/// empty-alternative rejection, single-character normalization via
/// `_character_to_key`, `show = bool(description and show)`, and `id`
/// propagation to every expanded entry.
fn expand_decl(decl: &BindingDecl) -> Vec<Result<BindingDecl, InvalidBinding>> {
    decl.key
        .split(',')
        .map(|alt| {
            let alt = alt.trim();
            if alt.is_empty() {
                return Err(InvalidBinding(format!(
                    "Can not bind empty string in {:?}",
                    decl.key
                )));
            }
            let key = normalize_single_char_key(alt);
            Ok(BindingDecl {
                key,
                action: decl.action.clone(),
                description: decl.description.clone(),
                tooltip: decl.tooltip.clone(),
                namespace: decl.namespace.clone(),
                show: !decl.description.is_empty() && decl.show,
                priority: decl.priority,
                id: decl.id.clone(),
            })
        })
        .collect()
}

/// Normalize a single-character key alternative to its canonical long name
/// (`"?"` -> `"question_mark"`), mirroring Python `_character_to_key` which
/// passes alphanumerics through unchanged.
fn normalize_single_char_key(alt: &str) -> String {
    let mut chars = alt.chars();
    match (chars.next(), chars.next()) {
        (Some(ch), None) if !ch.is_alphanumeric() => crate::keys::character_to_key_name(ch),
        _ => alt.to_string(),
    }
}

/// Manage a set of bindings as an ordered `key -> Vec<BindingDecl>` map
/// (Python `BindingsMap`).
///
/// Order fidelity matters beyond insertion order: `apply_keymap` ends with a
/// Python-`dict.update` step whose positional semantics are replicated by
/// [`BindingsMap::update_entries`] (existing keys keep their position and get
/// their value replaced; new keys append at the end; a key deleted
/// mid-algorithm and re-added lands at the end).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BindingsMap {
    /// Ordered mapping of key (e.g. `"ctrl+a"`) to bindings for that key.
    key_to_bindings: Vec<(String, Vec<BindingDecl>)>,
}

impl BindingsMap {
    /// Create an empty bindings map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct from declarative bindings, expanding comma lists into one
    /// entry per key (Python `BindingsMap.__init__` via `make_bindings`).
    ///
    /// Returns [`InvalidBinding`] on an empty comma-list alternative; Python
    /// raises this at class-definition time, and `from_decls` is the earliest
    /// structured place in Rust.
    pub fn from_decls(
        decls: impl IntoIterator<Item = BindingDecl>,
    ) -> Result<Self, InvalidBinding> {
        let mut map = Self::new();
        for decl in decls {
            for expanded in expand_decl(&decl) {
                map.push_binding(expanded?);
            }
        }
        Ok(map)
    }

    /// Lenient constructor for the dispatch path: malformed alternatives are
    /// logged via the debug facility and skipped instead of erroring, so a
    /// bad declaration can never panic (or drop the whole node's bindings)
    /// mid-keypress.
    pub(crate) fn from_decls_lossy(decls: impl IntoIterator<Item = BindingDecl>) -> Self {
        let mut map = Self::new();
        for decl in decls {
            for expanded in expand_decl(&decl) {
                match expanded {
                    Ok(binding) => map.push_binding(binding),
                    Err(err) => {
                        crate::debug::debug_input(&format!(
                            "[bindings] skipping invalid binding alternative: {err}"
                        ));
                    }
                }
            }
        }
        map
    }

    /// Merge several bindings maps, key-wise concatenating binding lists
    /// (Python `BindingsMap.merge`). The first occurrence of a key fixes its
    /// position.
    pub fn merge(maps: impl IntoIterator<Item = BindingsMap>) -> BindingsMap {
        let mut merged = BindingsMap::new();
        for map in maps {
            for (key, bindings) in map.key_to_bindings {
                match merged.position(&key) {
                    Some(idx) => merged.key_to_bindings[idx].1.extend(bindings),
                    None => merged.key_to_bindings.push((key, bindings)),
                }
            }
        }
        merged
    }

    /// The ordered `(key, bindings)` entries (Python exposes
    /// `key_to_bindings` publicly; this is the read-only Rust analog).
    pub fn entries(&self) -> &[(String, Vec<BindingDecl>)] {
        &self.key_to_bindings
    }

    /// Get the bindings for a key, or a typed [`NoBinding`] error on a miss
    /// (Python `get_bindings_for_key`, which raises).
    pub fn get_bindings_for_key(&self, key: &str) -> Result<&[BindingDecl], NoBinding> {
        self.get(key)
            .map(Vec::as_slice)
            .ok_or_else(|| NoBinding {
                key: key.to_string(),
            })
    }

    /// Bindings with `show == true`, in map order (Python `shown_keys`).
    pub fn shown_keys(&self) -> Vec<&BindingDecl> {
        self.key_to_bindings
            .iter()
            .flat_map(|(_, bindings)| bindings.iter())
            .filter(|binding| binding.show)
            .collect()
    }

    /// Flatten back to a `Vec<BindingDecl>` in map order (each entry
    /// single-key). Used by the dispatch-path transform.
    pub(crate) fn into_flattened(self) -> Vec<BindingDecl> {
        self.key_to_bindings
            .into_iter()
            .flat_map(|(_, bindings)| bindings)
            .collect()
    }

    /// Replace bindings for keys that are present in `keymap`, preserving
    /// existing bindings for keys that are not (Python
    /// `BindingsMap.apply_keymap`, ported faithfully).
    ///
    /// Notable pinned behaviors:
    /// - keymap values are split with a raw `split(',')` — NO trim and NO
    ///   character normalization (normalization is `App::set_keymap`'s job);
    ///   a value `"a, b"` inserts a key `" b"` with a leading space,
    /// - a binding occupying an override key clashes UNLESS it is itself
    ///   being rebound away by the keymap,
    /// - the self-clash + duplicate-append behavior of comma-expanded
    ///   sibling entries sharing one id is reproduced exactly (see the
    ///   `apply_keymap_self_clash_reports_remapped_binding_and_duplicates`
    ///   test; do not "fix" it).
    pub fn apply_keymap(&mut self, keymap: &Keymap) -> KeymapApplyResult {
        let mut clashed_bindings: Vec<BindingDecl> = Vec::new();
        let mut new_bindings: Vec<(String, Vec<BindingDecl>)> = Vec::new();

        let snapshot = self.key_to_bindings.clone();
        for (_key, bindings) in &snapshot {
            for binding in bindings {
                // Bindings without an ID are irrelevant when applying a keymap.
                let Some(binding_id) = binding.id.as_deref().filter(|id| !id.is_empty()) else {
                    continue;
                };

                // If the keymap has an override for this binding ID (Python's
                // walrus check: an empty-string value is falsy and skipped).
                let Some(keymap_key_string) =
                    keymap.get(binding_id).filter(|keys| !keys.is_empty())
                else {
                    continue;
                };
                // Raw split: no strip, no normalization (binding.py:294).
                let keymap_keys: Vec<&str> = keymap_key_string.split(',').collect();

                // Remove the old binding: delete every snapshot key whose
                // binding list carries this id (binding.py:296-301; the
                // `key.strip()` mirrors Python's shadowed-variable strip).
                for (snap_key, snap_bindings) in &snapshot {
                    let stripped = snap_key.trim();
                    if snap_bindings
                        .iter()
                        .any(|b| b.id.as_deref() == Some(binding_id))
                    {
                        self.remove_key(stripped);
                    }
                }

                for keymap_key in &keymap_keys {
                    if self.contains_key(keymap_key) || entries_get(&new_bindings, keymap_key).is_some()
                    {
                        // The key is already mapped either by default or by the
                        // keymap, so there's a clash unless the existing binding
                        // is being rebound to a different key (binding.py:315-321).
                        let mut clashing: Vec<BindingDecl> =
                            self.get(keymap_key).cloned().unwrap_or_default();
                        if let Some(pending) = entries_get(&new_bindings, keymap_key) {
                            clashing.extend(pending.iter().cloned());
                        }
                        for clashed_binding in clashing {
                            let rebound_away = clashed_binding
                                .id
                                .as_deref()
                                .filter(|id| !id.is_empty())
                                .map(|id| keymap.get(id).map(String::as_str)
                                    != Some(clashed_binding.key.as_str()))
                                .unwrap_or(false);
                            if !rebound_away && !clashed_bindings.contains(&clashed_binding) {
                                clashed_bindings.push(clashed_binding);
                            }
                        }
                        self.remove_key(keymap_key);
                    }
                }

                for keymap_key in &keymap_keys {
                    let remapped = BindingDecl {
                        key: (*keymap_key).to_string(),
                        ..binding.clone()
                    };
                    entries_push(&mut new_bindings, keymap_key, remapped);
                }
            }
        }

        self.update_entries(new_bindings);

        // Python returns a set; sort by (key, id) for stable assertions.
        clashed_bindings.sort_by(|a, b| (&a.key, &a.id).cmp(&(&b.key, &b.id)));
        KeymapApplyResult { clashed_bindings }
    }

    // -- ordered-map primitives (Python-dict positional semantics) ----------

    fn position(&self, key: &str) -> Option<usize> {
        self.key_to_bindings.iter().position(|(k, _)| k == key)
    }

    fn get(&self, key: &str) -> Option<&Vec<BindingDecl>> {
        self.position(key).map(|idx| &self.key_to_bindings[idx].1)
    }

    fn contains_key(&self, key: &str) -> bool {
        self.position(key).is_some()
    }

    /// Delete a key entry; remaining keys keep their original order
    /// (Python `del dict[key]`).
    fn remove_key(&mut self, key: &str) {
        if let Some(idx) = self.position(key) {
            self.key_to_bindings.remove(idx);
        }
    }

    /// Append a binding under its key (`dict.setdefault(key, []).append(..)`).
    fn push_binding(&mut self, binding: BindingDecl) {
        match self.position(&binding.key) {
            Some(idx) => self.key_to_bindings[idx].1.push(binding),
            None => {
                let key = binding.key.clone();
                self.key_to_bindings.push((key, vec![binding]));
            }
        }
    }

    /// Python `dict.update(other)` positional semantics: a key that still
    /// exists keeps its ORIGINAL position and gets its value replaced; a key
    /// that does not exist appends at the END (in `other` order).
    fn update_entries(&mut self, other: Vec<(String, Vec<BindingDecl>)>) {
        for (key, bindings) in other {
            match self.position(&key) {
                Some(idx) => self.key_to_bindings[idx].1 = bindings,
                None => self.key_to_bindings.push((key, bindings)),
            }
        }
    }
}

/// Lookup in a plain ordered-entry list (the `new_bindings` accumulator).
fn entries_get<'a>(
    entries: &'a [(String, Vec<BindingDecl>)],
    key: &str,
) -> Option<&'a Vec<BindingDecl>> {
    entries
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, bindings)| bindings)
}

/// `dict.setdefault(key, []).append(binding)` on an ordered-entry list.
fn entries_push(entries: &mut Vec<(String, Vec<BindingDecl>)>, key: &str, binding: BindingDecl) {
    match entries.iter_mut().find(|(k, _)| k == key) {
        Some((_, bindings)) => bindings.push(binding),
        None => entries.push((key.to_string(), vec![binding])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtures mirroring test_binding.py's BINDING1/BINDING2/BINDING3.
    fn binding1() -> BindingDecl {
        BindingDecl::new("a,b", "action1", "description1")
    }
    fn binding2() -> BindingDecl {
        BindingDecl::new("c", "action2", "description2")
    }
    fn binding3() -> BindingDecl {
        BindingDecl::new(" d   , e ", "action3", "description3")
    }

    fn bindings() -> BindingsMap {
        BindingsMap::from_decls([binding1(), binding2()]).expect("valid bindings")
    }

    fn more_bindings() -> BindingsMap {
        BindingsMap::from_decls([binding1(), binding2(), binding3()]).expect("valid bindings")
    }

    // Port of test_bindings_get_key.
    #[test]
    fn bindings_get_key() {
        let map = bindings();
        assert_eq!(
            map.get_bindings_for_key("b").expect("binding for b"),
            &[BindingDecl::new("b", "action1", "description1")]
        );
        assert_eq!(
            map.get_bindings_for_key("c").expect("binding for c"),
            &[BindingDecl::new("c", "action2", "description2")]
        );
        assert_eq!(
            map.get_bindings_for_key("control+meta+alt+shift+super+hyper+t"),
            Err(NoBinding {
                key: "control+meta+alt+shift+super+hyper+t".to_string()
            })
        );
    }

    // Port of test_bindings_get_key_spaced_list.
    #[test]
    fn bindings_get_key_spaced_list() {
        let map = more_bindings();
        assert_eq!(
            map.get_bindings_for_key("d").expect("binding for d")[0].action,
            map.get_bindings_for_key("e").expect("binding for e")[0].action,
        );
    }

    // Port of test_bindings_merge_simple.
    #[test]
    fn bindings_merge_simple() {
        let left = BindingsMap::from_decls([binding1()]).expect("valid");
        let right = BindingsMap::from_decls([binding2()]).expect("valid");
        assert_eq!(BindingsMap::merge([left, right]), bindings());
    }

    // Port of test_bindings_merge_overlap.
    #[test]
    fn bindings_merge_overlap() {
        let left = BindingsMap::from_decls([binding1()]).expect("valid");
        let another = BindingDecl::new("a", "another_action", "another_description");
        let merged = BindingsMap::merge([
            left,
            BindingsMap::from_decls([another.clone()]).expect("valid"),
        ]);
        assert_eq!(
            merged.entries(),
            &[
                (
                    "a".to_string(),
                    vec![BindingDecl::new("a", "action1", "description1"), another]
                ),
                (
                    "b".to_string(),
                    vec![BindingDecl::new("b", "action1", "description1")]
                ),
            ]
        );
    }

    // Port of test_binding_from_tuples (construction from parts).
    #[test]
    fn binding_from_parts() {
        let map = BindingsMap::from_decls([BindingDecl::new("c", "action2", "description2")])
            .expect("valid");
        assert_eq!(
            map.get_bindings_for_key("c").expect("binding for c"),
            &[binding2()]
        );
    }

    // Port of test_shown.
    #[test]
    fn shown_keys_filters_hidden() {
        let decls: Vec<BindingDecl> = ('a'..='z')
            .map(|key| {
                let mut decl = BindingDecl::new(
                    &key.to_string(),
                    &format!("action_{key}"),
                    &format!("Emits {key}"),
                );
                decl.show = (key as u32) % 2 == 1;
                decl
            })
            .collect();
        let map = BindingsMap::from_decls(decls).expect("valid");
        assert_eq!(map.shown_keys().len(), 13);
    }

    // Port of test_invalid_binding (",,," and ", ,").
    #[test]
    fn invalid_binding_empty_alternatives() {
        assert!(BindingsMap::from_decls([BindingDecl::new(",,,", "foo", "Broken")]).is_err());
        assert!(BindingsMap::from_decls([BindingDecl::new(", ,", "foo", "Broken")]).is_err());
    }

    // Lossy construction skips only the malformed alternatives.
    #[test]
    fn lossy_construction_skips_invalid_alternatives() {
        let map = BindingsMap::from_decls_lossy([
            BindingDecl::new(",,,", "foo", "Broken"),
            BindingDecl::new("a", "bar", "Fine"),
        ]);
        assert_eq!(map.entries().len(), 1);
        assert!(map.get_bindings_for_key("a").is_ok());
    }

    // Id propagation across comma expansion (all expanded keys share the id).
    #[test]
    fn id_propagates_across_comma_expansion() {
        let map = BindingsMap::from_decls([
            BindingDecl::new("i,up", "increment", "").with_id("app.increment"),
        ])
        .expect("valid");
        for key in ["i", "up"] {
            let entry = &map.get_bindings_for_key(key).expect("binding")[0];
            assert_eq!(entry.id.as_deref(), Some("app.increment"));
        }
    }

    // Single non-alphanumeric characters normalize to long key names;
    // alphanumerics pass through unchanged (Python _character_to_key).
    #[test]
    fn from_decls_normalizes_single_characters() {
        let map =
            BindingsMap::from_decls([BindingDecl::new("?,a", "help", "Help")]).expect("valid");
        assert!(map.get_bindings_for_key("question_mark").is_ok());
        assert!(map.get_bindings_for_key("a").is_ok());
        assert!(map.get_bindings_for_key("?").is_err());
    }

    // show = bool(description and show) (binding.py:161).
    #[test]
    fn expansion_applies_description_show_rule() {
        let map = BindingsMap::from_decls([BindingDecl::new("x", "act", "")]).expect("valid");
        assert!(!map.get_bindings_for_key("x").expect("binding")[0].show);
    }

    // -- ordered-map positional semantics (spec 3.2: test directly) ---------

    #[test]
    fn update_entries_replaces_in_place_and_appends_new() {
        let mut map = BindingsMap::from_decls([
            BindingDecl::new("a", "one", "d"),
            BindingDecl::new("b", "two", "d"),
            BindingDecl::new("c", "three", "d"),
        ])
        .expect("valid");

        // Existing key keeps its ORIGINAL position, value replaced; new key
        // appends at the end.
        map.update_entries(vec![
            ("b".to_string(), vec![BindingDecl::new("b", "TWO", "d")]),
            ("z".to_string(), vec![BindingDecl::new("z", "zed", "d")]),
        ]);
        let keys: Vec<&str> = map.entries().iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["a", "b", "c", "z"]);
        assert_eq!(map.get_bindings_for_key("b").expect("binding")[0].action, "TWO");
    }

    #[test]
    fn delete_then_update_readds_at_end() {
        let mut map = BindingsMap::from_decls([
            BindingDecl::new("a", "one", "d"),
            BindingDecl::new("b", "two", "d"),
            BindingDecl::new("c", "three", "d"),
        ])
        .expect("valid");

        // A key deleted mid-algorithm and then re-added lands at the END,
        // not at its old position (Python dict semantics).
        map.remove_key("a");
        let keys: Vec<&str> = map.entries().iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["b", "c"]);

        map.update_entries(vec![(
            "a".to_string(),
            vec![BindingDecl::new("a", "ONE", "d")],
        )]);
        let keys: Vec<&str> = map.entries().iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["b", "c", "a"]);
    }

    // -- apply_keymap ports --------------------------------------------------

    fn keymap(pairs: &[(&str, &str)]) -> Keymap {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // Keymap replaces the default keys wholesale (test_keymap replace, at the
    // value-type level).
    #[test]
    fn apply_keymap_replaces_default_keys() {
        let mut map = BindingsMap::from_decls([
            BindingDecl::new("i,up", "increment", "").with_id("app.increment"),
            BindingDecl::new("d,down", "decrement", "").with_id("app.decrement"),
        ])
        .expect("valid");

        let result = map.apply_keymap(&keymap(&[("app.increment", "right,k")]));
        assert!(result.clashed_bindings.is_empty());

        assert!(map.get_bindings_for_key("i").is_err());
        assert!(map.get_bindings_for_key("up").is_err());
        assert_eq!(
            map.get_bindings_for_key("right").expect("binding")[0].action,
            "increment"
        );
        assert_eq!(
            map.get_bindings_for_key("k").expect("binding")[0].action,
            "increment"
        );
        // Unrelated bindings preserved.
        assert!(map.get_bindings_for_key("d").is_ok());
        assert!(map.get_bindings_for_key("down").is_ok());
    }

    // Unknown ids are a no-op.
    #[test]
    fn apply_keymap_unknown_id_is_noop() {
        let mut map = BindingsMap::from_decls([
            BindingDecl::new("i,up", "increment", "").with_id("app.increment"),
        ])
        .expect("valid");
        let before = map.clone();
        let result = map.apply_keymap(&keymap(&[("this.is.an.unknown.id", "d")]));
        assert!(result.clashed_bindings.is_empty());
        assert_eq!(map, before);
    }

    // The pinned self-clash + duplicate-append quirk (spec 2.3): keymap
    // {app.increment: "d"} over BINDINGS "i,up"->increment / "d,down"->decrement
    // reports the REMAPPED increment binding as the single clash, and ends with
    // the increment binding stored TWICE under "d". Do not "fix" either.
    #[test]
    fn apply_keymap_self_clash_reports_remapped_binding_and_duplicates() {
        let mut map = BindingsMap::from_decls([
            BindingDecl::new("i,up", "increment", "").with_id("app.increment"),
            BindingDecl::new("d,down", "decrement", "").with_id("app.decrement"),
        ])
        .expect("valid");

        let result = map.apply_keymap(&keymap(&[("app.increment", "d")]));

        assert_eq!(result.clashed_bindings.len(), 1);
        let clash = &result.clashed_bindings[0];
        assert_eq!(clash.key, "d");
        assert_eq!(clash.action, "increment");
        assert_eq!(clash.id.as_deref(), Some("app.increment"));

        // Duplicate append: "d" holds the increment binding twice.
        let d_bindings = map.get_bindings_for_key("d").expect("binding");
        assert_eq!(d_bindings.len(), 2);
        assert!(d_bindings.iter().all(|b| b.action == "increment"));
        // The decrement binding lost its "d" entry but keeps "down".
        assert_eq!(
            map.get_bindings_for_key("down").expect("binding")[0].action,
            "decrement"
        );
    }

    // Raw split(',') of keymap values: NO strip and NO normalization; a value
    // "a, b" inserts a key " b" with a leading space (binding.py:294).
    // Normalization is set_keymap's job alone.
    #[test]
    fn apply_keymap_raw_splits_values_without_strip_or_normalize() {
        let mut map = BindingsMap::from_decls([
            BindingDecl::new("x", "act", "").with_id("act.id"),
        ])
        .expect("valid");
        map.apply_keymap(&keymap(&[("act.id", "a, b")]));
        assert!(map.get_bindings_for_key("a").is_ok());
        assert!(map.get_bindings_for_key(" b").is_ok());
        assert!(map.get_bindings_for_key("b").is_err());
    }

    // A displaced binding that is itself being rebound away does NOT clash.
    #[test]
    fn apply_keymap_rebound_away_binding_does_not_clash() {
        let mut map = BindingsMap::from_decls([
            BindingDecl::new("a", "one", "").with_id("one.id"),
            BindingDecl::new("b", "two", "").with_id("two.id"),
        ])
        .expect("valid");
        // one -> b (displaces two), two -> c (rebound away): no clash.
        let result = map.apply_keymap(&keymap(&[("one.id", "b"), ("two.id", "c")]));
        assert!(result.clashed_bindings.is_empty());
        assert_eq!(map.get_bindings_for_key("b").expect("binding")[0].action, "one");
        assert_eq!(map.get_bindings_for_key("c").expect("binding")[0].action, "two");
    }

    // A displaced binding withOUT an id (not addressable by the keymap) IS a
    // clash (Python's guard: `not (clashed_binding.id and ...)`).
    #[test]
    fn apply_keymap_displaced_idless_binding_clashes() {
        let mut map = BindingsMap::from_decls([
            BindingDecl::new("a", "one", "").with_id("one.id"),
            BindingDecl::new("b", "two", ""),
        ])
        .expect("valid");
        let result = map.apply_keymap(&keymap(&[("one.id", "b")]));
        assert_eq!(result.clashed_bindings.len(), 1);
        assert_eq!(result.clashed_bindings[0].action, "two");
    }

    // An empty-string keymap value is falsy in Python's walrus check: no-op.
    #[test]
    fn apply_keymap_empty_value_is_noop() {
        let mut map = BindingsMap::from_decls([
            BindingDecl::new("a", "one", "").with_id("one.id"),
        ])
        .expect("valid");
        let before = map.clone();
        let result = map.apply_keymap(&keymap(&[("one.id", "")]));
        assert!(result.clashed_bindings.is_empty());
        assert_eq!(map, before);
    }
}
