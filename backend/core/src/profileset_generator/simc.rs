use once_cell::sync::Lazy;
use regex::Regex;

const BASE64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

// Compiled once at first use. Each of these is referenced from hot loops that
// previously re-compiled the regex on every call.
static ENCHANT_ID_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"enchant_id=\d+").unwrap());
static ENCHANT_ID_CAPTURE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"enchant_id=(\d+)").unwrap());
static GEM_ID_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"gem_id=\d+").unwrap());
static GEM_ID_CAPTURE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"gem_id=(\d+)").unwrap());
static ITEM_ID_CAPTURE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"id=(\d+)").unwrap());
static AFTER_ITEM_ID_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(,id=\d+)").unwrap());
static BONUS_ID_CAPTURE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"bonus_id=([0-9/:]+)").unwrap());

pub(super) fn extract_spec_id_from_talent_string(talent_str: &str) -> Option<u64> {
    let mut bits = Vec::new();
    for ch in talent_str.bytes() {
        let val = BASE64.iter().position(|&b| b == ch)?;
        for bit in 0..6 {
            bits.push((val >> bit) & 1);
        }
        if bits.len() >= 24 {
            break;
        }
    }
    if bits.len() < 24 {
        return None;
    }
    let mut spec_id = 0u64;
    for i in 0..16 {
        if bits[8 + i] == 1 {
            spec_id |= 1 << i;
        }
    }
    Some(spec_id)
}

pub(super) fn set_enchant_id(simc: &str, enchant_id: u64) -> String {
    if ENCHANT_ID_RE.is_match(simc) {
        ENCHANT_ID_RE
            .replace(simc, &format!("enchant_id={}", enchant_id))
            .to_string()
    } else {
        AFTER_ITEM_ID_RE
            .replace(simc, &format!("$1,enchant_id={}", enchant_id))
            .to_string()
    }
}

pub(super) fn set_gem_id(simc: &str, gem_id: u64) -> String {
    if GEM_ID_RE.is_match(simc) {
        GEM_ID_RE
            .replace(simc, &format!("gem_id={}", gem_id))
            .to_string()
    } else {
        AFTER_ITEM_ID_RE
            .replace(simc, &format!("$1,gem_id={}", gem_id))
            .to_string()
    }
}

pub(super) fn extract_enchant_id(simc: &str) -> u64 {
    ENCHANT_ID_CAPTURE_RE
        .captures(simc)
        .and_then(|c| c[1].parse().ok())
        .unwrap_or(0)
}

pub(super) fn combinations<T: Clone>(items: &[T], k: usize) -> Vec<Vec<T>> {
    if k == 0 {
        return vec![vec![]];
    }
    if items.len() < k {
        return vec![];
    }
    let mut result = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let rest = combinations(&items[i + 1..], k - 1);
        for mut sub in rest {
            sub.insert(0, item.clone());
            result.push(sub);
        }
    }
    result
}

pub(super) fn simc_has_socket(simc: &str) -> bool {
    if extract_gem_id(simc) > 0 {
        return true;
    }
    let bonus_ids: Vec<u64> = BONUS_ID_CAPTURE_RE
        .captures(simc)
        .map(|c| {
            c[1].split(&['/', ':'][..])
                .filter_map(|s| s.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    let resolved = crate::item_db::resolve_bonuses(&bonus_ids);
    resolved.sockets.unwrap_or(0) > 0
}

pub(super) fn is_diamond(gem_item_id: u64) -> bool {
    crate::item_db::enchants_by_item_id()
        .get(&gem_item_id)
        .and_then(|v| v.get("quality"))
        .and_then(|q| q.as_u64())
        .map(|q| q == 4)
        .unwrap_or(false)
}

pub(super) fn gem_color(gem_item_id: u64) -> Option<String> {
    crate::item_db::enchants_by_item_id()
        .get(&gem_item_id)
        .and_then(|v| v.get("algariColor"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
}

pub(super) fn extract_item_id(simc: &str) -> u64 {
    ITEM_ID_CAPTURE_RE
        .captures(simc)
        .and_then(|c| c[1].parse().ok())
        .unwrap_or(0)
}

pub(super) fn extract_gem_id(simc: &str) -> u64 {
    GEM_ID_CAPTURE_RE
        .captures(simc)
        .and_then(|c| c[1].parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Once;

    static LOAD_GAME_DATA: Once = Once::new();
    fn ensure_game_data_loaded() {
        LOAD_GAME_DATA.call_once(|| {
            let data_dir =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../resources/data-compacted");
            crate::item_db::load(&data_dir);
        });
    }

    #[test]
    fn set_enchant_id_replaces_existing() {
        let s = ",id=100,enchant_id=7777,bonus_id=12";
        assert_eq!(set_enchant_id(s, 8888), ",id=100,enchant_id=8888,bonus_id=12");
    }

    #[test]
    fn set_enchant_id_inserts_after_item_id_when_missing() {
        let s = ",id=100,bonus_id=12";
        assert_eq!(set_enchant_id(s, 8888), ",id=100,enchant_id=8888,bonus_id=12");
    }

    #[test]
    fn set_gem_id_replaces_existing() {
        let s = ",id=100,gem_id=5555,bonus_id=12";
        assert_eq!(set_gem_id(s, 6666), ",id=100,gem_id=6666,bonus_id=12");
    }

    #[test]
    fn set_gem_id_inserts_after_item_id_when_missing() {
        let s = ",id=100,bonus_id=12";
        assert_eq!(set_gem_id(s, 6666), ",id=100,gem_id=6666,bonus_id=12");
    }

    #[test]
    fn extract_enchant_id_returns_zero_when_missing() {
        assert_eq!(extract_enchant_id(",id=100,bonus_id=12"), 0);
    }

    #[test]
    fn extract_enchant_id_returns_value_when_present() {
        assert_eq!(extract_enchant_id(",id=100,enchant_id=7777"), 7777);
    }

    #[test]
    fn extract_gem_id_returns_zero_when_missing() {
        assert_eq!(extract_gem_id(",id=100"), 0);
    }

    #[test]
    fn extract_gem_id_returns_value_when_present() {
        assert_eq!(extract_gem_id(",id=100,gem_id=5555"), 5555);
    }

    #[test]
    fn extract_item_id_returns_value() {
        assert_eq!(extract_item_id(",id=151336,enchant_id=8017"), 151336);
    }

    #[test]
    fn simc_has_socket_true_when_gem_present() {
        assert!(simc_has_socket(",id=100,gem_id=12345"));
    }

    #[test]
    fn simc_has_socket_true_for_socket_adding_bonus() {
        ensure_game_data_loaded();
        // Bonus 13534 adds 1 socket per data-compacted/bonuses.json
        assert!(simc_has_socket(",id=100,bonus_id=13534"));
    }

    #[test]
    fn simc_has_socket_false_without_socket() {
        ensure_game_data_loaded();
        assert!(!simc_has_socket(",id=100"));
        assert!(!simc_has_socket(",id=100,bonus_id=13440")); // 13440 is a tag bonus, no socket
    }

    #[test]
    fn combinations_empty_set() {
        let items: Vec<u32> = vec![];
        assert_eq!(combinations(&items, 0), vec![Vec::<u32>::new()]);
    }

    #[test]
    fn combinations_k_zero_returns_one_empty() {
        let items = vec![1, 2, 3];
        assert_eq!(combinations(&items, 0), vec![Vec::<i32>::new()]);
    }

    #[test]
    fn combinations_k_greater_than_n_returns_empty() {
        let items = vec![1, 2];
        let result: Vec<Vec<i32>> = combinations(&items, 3);
        assert!(result.is_empty());
    }

    #[test]
    fn combinations_basic_case() {
        let items = vec![1, 2, 3];
        let result = combinations(&items, 2);
        assert_eq!(result, vec![vec![1, 2], vec![1, 3], vec![2, 3]]);
    }

    #[test]
    fn is_diamond_true_for_quality_4_gem() {
        ensure_game_data_loaded();
        // 213738 is a diamond/prismatic gem in the test fixture
        assert!(is_diamond(213738));
    }

    #[test]
    fn is_diamond_false_for_normal_gem() {
        ensure_game_data_loaded();
        assert!(!is_diamond(213453));
    }

    #[test]
    fn is_diamond_false_for_unknown_id() {
        ensure_game_data_loaded();
        assert!(!is_diamond(99999999));
    }

    #[test]
    fn extract_spec_id_from_talent_string_returns_some() {
        // A known subtlety talent string from the user's report
        let talent = "CUQAphyM11FofNMFa1K3vFEDUCgx2MAAAAAwsMGLTMbbjxMjZMMzMzYMbzYGbLzMzMzMjBjZ2GAAAAGMGwYWMMwAziWoFbYGwMDmxA";
        assert!(extract_spec_id_from_talent_string(talent).is_some());
    }

    #[test]
    fn extract_spec_id_from_talent_string_returns_none_for_short_input() {
        assert!(extract_spec_id_from_talent_string("AB").is_none());
    }
}
