//! Suggester cache behavior (port of Python tests/suggester/test_suggester.py).
//!
//! The Rust `Suggester::suggest` entry point is synchronous and returns the
//! suggestion directly, so the Python `SuggestionReady`-message tests have no
//! equivalent here (async delivery is a separate, deferred item). These tests
//! cover the caching and case-normalization contract shared with Python's
//! `Suggester._get_suggestion`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use textual::widgets::{SuggestFromList, Suggester, SuggestionCache};

/// A suggester that records every `get_suggestion` computation.
struct CountingSuggester {
    log: Mutex<Vec<String>>,
    cache: Option<SuggestionCache>,
}

impl CountingSuggester {
    fn new(use_cache: bool) -> Self {
        Self {
            log: Mutex::new(Vec::new()),
            cache: use_cache.then(SuggestionCache::new),
        }
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

impl Suggester for CountingSuggester {
    fn get_suggestion(&self, value: &str) -> Option<String> {
        self.log.lock().unwrap().push(value.to_string());
        Some(value.to_string())
    }

    fn cache(&self) -> Option<&SuggestionCache> {
        self.cache.as_ref()
    }
}

// Port of test_cache_on: a caching suggester consulted twice for the same
// input computes only once.
#[test]
fn suggester_cache_on() {
    let suggester = CountingSuggester::new(true);
    assert_eq!(suggester.suggest("hello").as_deref(), Some("hello"));
    assert_eq!(suggester.log(), vec!["hello"]);
    assert_eq!(suggester.suggest("hello").as_deref(), Some("hello"));
    assert_eq!(suggester.log(), vec!["hello"]);
}

// Port of test_cache_off: without a cache, every consultation computes.
#[test]
fn suggester_cache_off() {
    let suggester = CountingSuggester::new(false);
    suggester.suggest("hello");
    assert_eq!(suggester.log(), vec!["hello"]);
    suggester.suggest("hello");
    assert_eq!(suggester.log(), vec!["hello", "hello"]);
}

// Port of test_case_insensitive_suggestions: when not case sensitive, values
// are normalized before reaching get_suggestion.
#[test]
fn suggester_case_insensitive_suggestions() {
    for value in ["hello", "HELLO", "HeLlO", "Hello", "hELLO"] {
        let suggester = CountingSuggester::new(false);
        suggester.suggest(value);
        assert_eq!(
            suggester.log(),
            vec!["hello"],
            "value {value:?} should be lowercased before get_suggestion"
        );
    }
}

// Port of test_case_insensitive_cache_hits: differently-cased inputs share
// one cache entry.
#[test]
fn suggester_case_insensitive_cache_hits() {
    struct Counter {
        count: AtomicUsize,
        cache: SuggestionCache,
    }
    impl Suggester for Counter {
        fn get_suggestion(&self, value: &str) -> Option<String> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Some(format!("{value}abc"))
        }
        fn cache(&self) -> Option<&SuggestionCache> {
            Some(&self.cache)
        }
    }

    let suggester = Counter {
        count: AtomicUsize::new(0),
        cache: SuggestionCache::new(),
    };
    for hello in ["hello", "HELLO", "HeLlO", "Hello", "hELLO"] {
        assert_eq!(suggester.suggest(hello).as_deref(), Some("helloabc"));
    }
    assert_eq!(suggester.count.load(Ordering::SeqCst), 1);
}

// A cached "no suggestion" result is also a cache hit (Python caches None).
#[test]
fn suggester_caches_none_results() {
    struct NoneSuggester {
        count: AtomicUsize,
        cache: SuggestionCache,
    }
    impl Suggester for NoneSuggester {
        fn get_suggestion(&self, _value: &str) -> Option<String> {
            self.count.fetch_add(1, Ordering::SeqCst);
            None
        }
        fn cache(&self) -> Option<&SuggestionCache> {
            Some(&self.cache)
        }
    }

    let suggester = NoneSuggester {
        count: AtomicUsize::new(0),
        cache: SuggestionCache::new(),
    };
    assert_eq!(suggester.suggest("hello"), None);
    assert_eq!(suggester.suggest("hello"), None);
    assert_eq!(suggester.count.load(Ordering::SeqCst), 1);
}

// SuggestFromList still suggests correctly through the caching entry point,
// preserving canonical casing, and honors use_cache(false).
#[test]
fn suggest_from_list_through_cache() {
    let countries = ["England", "Scotland", "Portugal", "Spain", "France"];

    let suggester = SuggestFromList::new(countries, false);
    assert_eq!(suggester.suggest("por").as_deref(), Some("Portugal"));
    // Second consultation (cache hit) returns the same canonical value.
    assert_eq!(suggester.suggest("por").as_deref(), Some("Portugal"));
    assert_eq!(suggester.suggest("POR").as_deref(), Some("Portugal"));
    assert_eq!(suggester.suggest("zzz"), None);

    // Case sensitive: needle must match canonical casing.
    let sensitive = SuggestFromList::new(countries, true);
    assert_eq!(sensitive.suggest("Por").as_deref(), Some("Portugal"));
    assert_eq!(sensitive.suggest("por"), None);

    // Cache disabled still computes correctly.
    let uncached = SuggestFromList::new(countries, false).use_cache(false);
    assert!(uncached.cache().is_none());
    assert_eq!(uncached.suggest("sc").as_deref(), Some("Scotland"));
    assert_eq!(uncached.suggest("sc").as_deref(), Some("Scotland"));
}
