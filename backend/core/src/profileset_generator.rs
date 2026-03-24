use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::addon_parser::GEAR_SLOTS;
use crate::game_data;

/// Armor-type-restricted slots (head, shoulder, chest, wrist, hands, waist, legs, feet).
/// Slots like neck, back, finger, trinket, and weapons are NOT armor-type restricted.
const ARMOR_SLOTS: &[&str] = &[
    "head", "shoulder", "chest", "wrist", "hands", "waist", "legs", "feet",
];

/// Returns the maximum armor subclass for a given WoW class name.
/// 1=Cloth, 2=Leather, 3=Mail, 4=Plate.
/// Classes can wear their type and anything lighter (e.g. Mail can also wear Leather/Cloth).
fn class_max_armor_subclass(class_name: &str) -> Option<u64> {
    match class_name.to_lowercase().as_str() {
        "priest" | "mage" | "warlock" => Some(1),
        "rogue" | "monk" | "druid" | "demon_hunter" | "demonhunter" => Some(2),
        "hunter" | "shaman" | "evoker" => Some(3),
        "warrior" | "paladin" | "death_knight" | "deathknight" => Some(4),
        _ => None,
    }
}

/// Parse the character class from a base profile string.
/// Looks for the class="Name" line, e.g. `warrior="Sørtbek"`.
fn detect_class(base_profile: &str) -> Option<String> {
    let class_re = Regex::new(
        r#"^(warrior|paladin|hunter|rogue|priest|death_knight|deathknight|shaman|mage|warlock|monk|demon_hunter|demonhunter|druid|evoker)\s*="#
    ).unwrap();
    for line in base_profile.lines() {
        let trimmed = line.trim();
        if let Some(caps) = class_re.captures(trimmed) {
            return Some(caps[1].to_string());
        }
    }
    None
}

use once_cell::sync::Lazy;

/// Maximum gear combinations for Top Gear. Override with MAX_COMBINATIONS env var.
pub static MAX_COMBINATIONS: Lazy<usize> = Lazy::new(|| {
    if let Ok(val) = std::env::var("MAX_COMBINATIONS") {
        if let Ok(n) = val.parse() { return n; }
    }
    500
});

const UNIQUE_SLOT_PAIRS: &[(&str, &str)] = &[
    ("finger1", "finger2"),
    ("trinket1", "trinket2"),
];

/// Generate a simc input string with full-set profilesets for Top Gear.
///
/// Returns (simc_input_string, combination_count, combo_metadata).
/// combo_metadata maps "Combo N" -> list of item metadata values.
pub fn generate_top_gear_input(
    base_profile: &str,
    items_by_slot: &HashMap<String, Vec<Value>>,
    selected_items: &HashMap<String, Vec<usize>>,
    max_combos_override: Option<usize>,
) -> Result<(String, usize, HashMap<String, Vec<Value>>), String> {
    // Extract base profile info (non-gear lines) and equipped gear
    let (base_lines, equipped_gear, talents_string, _spec) = parse_base_profile(base_profile);

    // Build the option lists per slot for combination generation
    let mut slot_item_lists: HashMap<String, Vec<Value>> = HashMap::new();

    for slot in GEAR_SLOTS {
        let slot = slot.to_string();
        let slot_items = match items_by_slot.get(&slot) {
            Some(items) => items,
            None => continue,
        };

        let selected_indices = selected_items.get(&slot).cloned().unwrap_or_default();

        // Collect all selected items for this slot
        let mut candidates: Vec<Value> = Vec::new();
        for &idx in &selected_indices {
            if idx < slot_items.len() {
                candidates.push(slot_items[idx].clone());
            }
        }

        // Also always include the equipped item if not already selected
        let equipped = slot_items
            .iter()
            .find(|it| it.get("is_equipped").and_then(|v| v.as_bool()).unwrap_or(false));

        if let Some(eq) = equipped {
            let already_included = candidates.iter().any(|c| {
                // Compare by pointer identity isn't possible with Value, compare item_id
                c.get("item_id") == eq.get("item_id")
                    && c.get("is_equipped").and_then(|v| v.as_bool()).unwrap_or(false)
            });
            if !already_included {
                candidates.insert(0, eq.clone());
            }
        }

        if !candidates.is_empty() {
            slot_item_lists.insert(slot, candidates);
        }
    }

    // Filter out items whose armor type the character's class can't equip.
    // Classes can wear their armor type and anything lighter (e.g. Mail can wear Leather/Cloth).
    if let Some(class_name) = detect_class(base_profile) {
        if let Some(max_subclass) = class_max_armor_subclass(&class_name) {
            for slot in ARMOR_SLOTS {
                let slot = slot.to_string();
                if let Some(items) = slot_item_lists.get_mut(&slot) {
                    items.retain(|item| {
                        // Always keep equipped items (already validated by the game)
                        if item.get("is_equipped").and_then(|v| v.as_bool()).unwrap_or(false) {
                            return true;
                        }
                        let item_id = item.get("item_id").and_then(|v| v.as_u64()).unwrap_or(0);
                        if item_id == 0 {
                            return true;
                        }
                        match game_data::get_item_armor_subclass(item_id) {
                            Some(subclass) => subclass <= max_subclass || subclass == 0, // 0 = Misc, always OK
                            None => true, // Item not found in DB, keep it
                        }
                    });
                }
            }
        }
    }

    // Find slots that have alternatives (more than just equipped)
    let varying_slots: Vec<String> = slot_item_lists
        .iter()
        .filter(|(_, items)| items.len() > 1)
        .map(|(slot, _)| slot.clone())
        .collect();

    // Sort for deterministic ordering
    let mut varying_slots = varying_slots;
    varying_slots.sort();

    if varying_slots.is_empty() {
        return Ok((base_profile.to_string(), 0, HashMap::new()));
    }

    // Build cartesian product across varying slots
    let option_lists: Vec<&Vec<Value>> = varying_slots
        .iter()
        .map(|slot| slot_item_lists.get(slot).unwrap())
        .collect();

    // Generate all combos via iterative cartesian product
    let mut all_combos: Vec<Vec<usize>> = vec![vec![]];
    for opts in &option_lists {
        let mut new_combos = Vec::new();
        for combo in &all_combos {
            for i in 0..opts.len() {
                let mut new = combo.clone();
                new.push(i);
                new_combos.push(new);
            }
        }
        all_combos = new_combos;
    }

    // Filter invalid combos and build gear sets
    let mut valid_combos: Vec<HashMap<String, Value>> = Vec::new();

    for combo_indices in &all_combos {
        // Build full gear set: start with equipped, override varying slots
        let mut gear_set: HashMap<String, Value> = HashMap::new();
        for slot in GEAR_SLOTS {
            let slot = slot.to_string();
            if let Some(items) = slot_item_lists.get(&slot) {
                // Use equipped item as default
                let default = items
                    .iter()
                    .find(|it| it.get("is_equipped").and_then(|v| v.as_bool()).unwrap_or(false))
                    .unwrap_or(&items[0]);
                gear_set.insert(slot, default.clone());
            }
        }

        // Apply the combo choices
        for (i, slot) in varying_slots.iter().enumerate() {
            let item = &option_lists[i][combo_indices[i]];
            gear_set.insert(slot.clone(), item.clone());
        }

        // Validate unique-equipped constraints
        if !validate_unique_equipped(&gear_set) {
            continue;
        }

        // Vault constraint: only one vault item can be picked
        if !validate_vault_constraint(&gear_set) {
            continue;
        }

        // Check if this is identical to baseline (all equipped)
        let is_baseline = GEAR_SLOTS.iter().all(|slot| {
            gear_set
                .get(*slot)
                .and_then(|item| item.get("is_equipped"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
        });
        if is_baseline {
            continue;
        }

        valid_combos.push(gear_set);
    }

    let combo_count = valid_combos.len();
    let limit = max_combos_override.unwrap_or(*MAX_COMBINATIONS);
    if combo_count > limit {
        return Err(format!(
            "Too many combinations ({}). Maximum is {}. Please deselect some items.",
            combo_count, limit
        ));
    }

    if combo_count == 0 {
        return Ok((base_profile.to_string(), 0, HashMap::new()));
    }

    // Build output: base profile as Combo 1, then profilesets
    let mut lines: Vec<String> = Vec::new();
    let mut combo_metadata: HashMap<String, Vec<Value>> = HashMap::new();

    // Write clean base profile (non-gear lines + equipped gear)
    lines.push("# Base Actor".to_string());
    lines.extend(base_lines.clone());
    lines.push("### Combo 1".to_string());
    for slot in GEAR_SLOTS {
        let slot_str = slot.to_string();
        if let Some(gear_val) = equipped_gear.get(&slot_str) {
            lines.push(format!("{}={}", slot, gear_val));
        } else if *slot == "off_hand" {
            lines.push("off_hand=,".to_string());
        }
    }
    if !talents_string.is_empty() {
        lines.push(format!("talents={}", talents_string));
    }
    lines.push(String::new());

    // Build baseline metadata for "Currently Equipped"
    let paired_display_slots = ["finger1", "finger2", "trinket1", "trinket2"];
    let mut baseline_items: Vec<Value> = Vec::new();
    for slot in &paired_display_slots {
        let slot = slot.to_string();
        if let Some(items) = slot_item_lists.get(&slot) {
            if !items.is_empty() {
                baseline_items.push(item_meta(&items[0], &slot));
            }
        }
    }
    combo_metadata.insert("Currently Equipped".to_string(), baseline_items);

    // Generate profilesets for each combo
    for (combo_idx, gear_set) in valid_combos.iter().enumerate() {
        let combo_name = format!("Combo {}", combo_idx + 2);
        lines.push(format!("### {}", combo_name));

        for slot in GEAR_SLOTS {
            let slot_str = slot.to_string();
            if let Some(item) = gear_set.get(&slot_str) {
                let simc_str = item
                    .get("simc_string")
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                lines.push(format!(
                    "profileset.\"{}\"+={}={}",
                    combo_name, slot, simc_str
                ));
            } else if *slot == "off_hand" {
                lines.push(format!("profileset.\"{}\"+=off_hand=,", combo_name));
            }
        }

        if !talents_string.is_empty() {
            lines.push(format!(
                "profileset.\"{}\"+=talents={}",
                combo_name, talents_string
            ));
        }
        lines.push(String::new());

        // Build metadata: track paired slots + changed non-paired slots
        let mut combo_items: Vec<Value> = Vec::new();
        for slot in &paired_display_slots {
            let slot = slot.to_string();
            if let Some(item) = gear_set.get(&slot) {
                let mut meta = item_meta(item, &slot);
                meta["is_kept"] = json!(item
                    .get("is_equipped")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false));
                combo_items.push(meta);
            }
        }

        // Also include non-paired slots that changed
        for slot in GEAR_SLOTS {
            if paired_display_slots.contains(slot) {
                continue;
            }
            let slot_str = slot.to_string();
            if let Some(item) = gear_set.get(&slot_str) {
                let is_equipped = item
                    .get("is_equipped")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                if !is_equipped {
                    combo_items.push(item_meta(item, &slot_str));
                }
            }
        }

        combo_metadata.insert(combo_name, combo_items);
    }

    Ok((lines.join("\n"), combo_count, combo_metadata))
}

fn parse_base_profile(base_profile: &str) -> (Vec<String>, HashMap<String, String>, String, String) {
    let mut non_gear_lines: Vec<String> = Vec::new();
    let mut equipped_gear: HashMap<String, String> = HashMap::new();
    let mut talents_string = String::new();
    let mut spec_string = String::new();

    let gear_pattern = format!(r"^({})=(.*)", GEAR_SLOTS.join("|"));
    let gear_re = Regex::new(&gear_pattern).unwrap();
    let talents_re = Regex::new(r"^talents=(.+)").unwrap();
    let spec_re = Regex::new(r"^spec=(\w+)").unwrap();

    for line in base_profile.lines() {
        let stripped = line.trim();
        if stripped.is_empty() {
            continue;
        }

        // Extract talents
        if let Some(caps) = talents_re.captures(stripped) {
            talents_string = caps[1].to_string();
            continue;
        }

        // Extract spec
        if let Some(caps) = spec_re.captures(stripped) {
            spec_string = caps[1].to_lowercase();
        }

        // Extract gear lines
        if let Some(caps) = gear_re.captures(stripped) {
            let slot = caps[1].to_lowercase();
            let value = caps[2].to_string();
            equipped_gear.insert(slot, value);
            continue;
        }

        // Keep everything else
        non_gear_lines.push(stripped.to_string());
    }

    (non_gear_lines, equipped_gear, talents_string, spec_string)
}

fn item_meta(item: &Value, slot: &str) -> Value {
    json!({
        "slot": slot,
        "item_id": item.get("item_id").and_then(|v| v.as_u64()).unwrap_or(0),
        "ilevel": item.get("ilevel").and_then(|v| v.as_u64()).unwrap_or(0),
        "name": item.get("name").and_then(|v| v.as_str()).unwrap_or(""),
        "bonus_ids": item.get("bonus_ids").cloned().unwrap_or(json!([])),
        "enchant_id": item.get("enchant_id").and_then(|v| v.as_u64()).unwrap_or(0),
        "gem_id": item.get("gem_id").and_then(|v| v.as_u64()).unwrap_or(0),
        "is_kept": item.get("is_equipped").and_then(|v| v.as_bool()).unwrap_or(false),
        "origin": item.get("origin").and_then(|v| v.as_str()).unwrap_or("bags"),
    })
}

/// Specs that can dual wield (equip one-hand weapons in both hands).
fn can_dual_wield(spec: &str) -> bool {
    matches!(
        spec,
        "fury" | "frost" | "enhancement" | "windwalker" | "brewmaster"
        | "havoc" | "vengeance"
        | "outlaw" | "assassination" | "subtlety"
    )
}

fn inv_type_to_slots(inv_type: u64, spec: &str) -> Vec<&'static str> {
    match inv_type {
        1 => vec!["head"], 2 => vec!["neck"], 3 => vec!["shoulder"],
        5 | 20 => vec!["chest"], 6 => vec!["waist"], 7 => vec!["legs"],
        8 => vec!["feet"], 9 => vec!["wrist"], 10 => vec!["hands"],
        11 => vec!["finger1", "finger2"],
        12 => vec!["trinket1", "trinket2"],
        13 => {
            if can_dual_wield(spec) {
                vec!["main_hand", "off_hand"]
            } else {
                vec!["main_hand"]
            }
        }
        14 => vec!["off_hand"],
        16 => vec!["back"],
        17 => {
            if spec == "fury" {
                vec!["main_hand", "off_hand"]
            } else {
                vec!["main_hand"]
            }
        }
        15 | 26 | 21 => vec!["main_hand"],
        22 | 23 => vec!["off_hand"],
        _ => vec![],
    }
}

pub fn generate_droptimizer_input(
    base_profile: &str,
    drop_items: &[Value],
) -> (String, usize, HashMap<String, Value>) {
    let (base_lines, equipped_gear, talents_string, spec) = parse_base_profile(base_profile);

    let mut lines: Vec<String> = Vec::new();
    let mut combo_metadata: HashMap<String, Value> = HashMap::new();

    // Write base profile
    lines.push("# Base Actor".to_string());
    lines.extend(base_lines);
    lines.push("### Combo 1".to_string());
    for slot in GEAR_SLOTS {
        if let Some(gear) = equipped_gear.get(*slot) {
            lines.push(format!("{}={}", slot, gear));
        } else if *slot == "off_hand" {
            lines.push("off_hand=,".to_string());
        }
    }
    if !talents_string.is_empty() {
        lines.push(format!("talents={}", talents_string));
    }
    lines.push(String::new());

    // Detect if currently using a two-hander (no off-hand or empty off-hand)
    let has_two_hand_equipped = {
        let oh = equipped_gear.get("off_hand").map(|s| s.trim());
        oh.is_none() || oh == Some("") || oh == Some(",")
    };

    // Extract enchant/runeforge from equipped gear to copy onto drop items.
    // Gems are NOT copied because drop items may not have sockets.
    let enchant_re = Regex::new(r"(enchant_id=\d+)").unwrap();

    let mut combo_idx = 2usize;
    for item in drop_items {
        let item_id = item.get("item_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let ilevel = item.get("ilevel").and_then(|v| v.as_u64()).unwrap_or(0);
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let encounter = item.get("encounter").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let inv_type = item.get("inventory_type").and_then(|v| v.as_u64()).unwrap_or(0);
        let bonus_ids: Vec<u64> = item.get("bonus_ids")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|b| b.as_u64()).collect())
            .unwrap_or_default();
        let mut slots = inv_type_to_slots(inv_type, &spec);

        // If the character has a two-hander equipped, nothing can go in the
        // off-hand — except two-handers for Fury warriors (Titan's Grip).
        if has_two_hand_equipped && !(spec == "fury" && inv_type == 17) {
            slots.retain(|s| *s != "off_hand");
        }

        if slots.is_empty() {
            continue;
        }

        let mut base_simc_str = format!(",id={},ilevel={}", item_id, ilevel);
        if !bonus_ids.is_empty() {
            let bonus_str = bonus_ids.iter().map(|b| b.to_string()).collect::<Vec<_>>().join("/");
            base_simc_str.push_str(&format!(",bonus_id={}", bonus_str));
        }

        for slot in &slots {
            // Copy enchants/gems from the currently equipped item in this slot
            let mut simc_str = base_simc_str.clone();
            if let Some(equipped) = equipped_gear.get(*slot) {
                if let Some(caps) = enchant_re.captures(equipped) {
                    simc_str.push_str(&format!(",{}", &caps[1]));
                }
            }

            let combo_name = format!("Combo {}", combo_idx);
            lines.push(format!("### {}", combo_name));
            lines.push(format!("profileset.\"{}\"+={}={}", combo_name, slot, simc_str));
            if inv_type == 17 && *slot == "main_hand" && spec != "fury" {
                lines.push(format!("profileset.\"{}\"+=off_hand=,", combo_name));
            }
            if !talents_string.is_empty() {
                lines.push(format!("profileset.\"{}\"+=talents={}", combo_name, talents_string));
            }
            lines.push(String::new());

            combo_metadata.insert(combo_name.clone(), json!([{
                "slot": slot,
                "item_id": item_id,
                "ilevel": ilevel,
                "name": name,
                "bonus_ids": bonus_ids,
                "enchant_id": 0,
                "gem_id": 0,
                "is_kept": false,
                "encounter": encounter,
            }]));
            combo_idx += 1;
        }
    }

    let combo_count = combo_idx - 2;
    (lines.join("\n"), combo_count, combo_metadata)
}

/// Vault constraint: at most one vault item across all slots.
fn validate_vault_constraint(gear_set: &HashMap<String, Value>) -> bool {
    let mut vault_count = 0;
    for item in gear_set.values() {
        if item.get("origin").and_then(|v| v.as_str()) == Some("vault") {
            vault_count += 1;
            if vault_count > 1 {
                return false;
            }
        }
    }
    true
}

fn validate_unique_equipped(gear_set: &HashMap<String, Value>) -> bool {
    for (slot1, slot2) in UNIQUE_SLOT_PAIRS {
        let item1 = gear_set.get(*slot1);
        let item2 = gear_set.get(*slot2);
        if let (Some(i1), Some(i2)) = (item1, item2) {
            let id1 = i1.get("item_id").and_then(|v| v.as_u64()).unwrap_or(0);
            let id2 = i2.get("item_id").and_then(|v| v.as_u64()).unwrap_or(0);
            if id1 != 0 && id2 != 0 && id1 == id2 {
                return false;
            }
        }
    }
    true
}
