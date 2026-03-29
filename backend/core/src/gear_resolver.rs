//! Gear Resolver — takes flat parsed items + character info + item DB
//! and returns a fully enriched, slot-resolved gear layout.
//!
//! This is the single authority for slot eligibility, armor filtering,
//! dual-wield crossover, deduplication, and item enrichment.

use std::collections::{HashMap, HashSet};

use crate::item_db;
use crate::types::class_data::{self, ARMOR_SLOTS, GEAR_SLOTS};
use crate::types::*;

/// Build a stable UID for deduplication: "item_id:sorted_bonus_ids:origin:raw_slot"
fn make_uid(item: &RawParsedItem) -> String {
    let mut sorted = item.bonus_ids.clone();
    sorted.sort();
    let bonus_key = sorted
        .iter()
        .map(|b| b.to_string())
        .collect::<Vec<_>>()
        .join(":");
    format!(
        "{}:{}:{}:{}",
        item.item_id,
        bonus_key,
        item.origin.as_str(),
        item.raw_slot
    )
}

/// Dedup key: item_id + sorted bonus_ids (ignores origin/slot).
fn dedup_key(item: &RawParsedItem) -> String {
    let mut sorted = item.bonus_ids.clone();
    sorted.sort();
    let bonus_key = sorted
        .iter()
        .map(|b| b.to_string())
        .collect::<Vec<_>>()
        .join(":");
    format!("{}:{}", item.item_id, bonus_key)
}

/// Enrich a raw item with display info from the item DB.
fn enrich(item: &RawParsedItem, slot: &str) -> ResolvedItem {
    let info = item_db::get_item_info(item.item_id, Some(&item.bonus_ids));

    let (name, icon, quality, tag, upgrade, sockets) = if let Some(ref info) = info {
        (
            info.get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("Unknown")
                .to_string(),
            info.get("icon")
                .and_then(|i| i.as_str())
                .unwrap_or("inv_misc_questionmark")
                .to_string(),
            info.get("quality").and_then(|q| q.as_u64()).unwrap_or(1),
            info.get("tag")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
            info.get("upgrade")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string(),
            info.get("sockets").and_then(|s| s.as_u64()).unwrap_or(0),
        )
    } else {
        let name = if item.name.is_empty() {
            format!("Item {}", item.item_id)
        } else {
            item.name.clone()
        };
        (
            name,
            "inv_misc_questionmark".to_string(),
            1,
            String::new(),
            String::new(),
            0,
        )
    };

    // Resolve ilevel: prefer DB-resolved (accounts for bonuses), fall back to parsed
    let ilevel = info
        .as_ref()
        .and_then(|i| i.get("ilevel").and_then(|v| v.as_u64()))
        .filter(|&v| v > 0)
        .unwrap_or(item.ilevel);

    let enchant_name = if item.enchant_id > 0 {
        item_db::get_enchant_info(item.enchant_id)
            .and_then(|e| {
                e.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default()
    } else {
        String::new()
    };

    let (gem_name, gem_icon) = if item.gem_id > 0 {
        item_db::get_gem_info(item.gem_id)
            .map(|g| {
                (
                    g.get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string(),
                    g.get("icon")
                        .and_then(|i| i.as_str())
                        .unwrap_or("")
                        .to_string(),
                )
            })
            .unwrap_or_default()
    } else {
        (String::new(), String::new())
    };

    ResolvedItem {
        uid: make_uid(item),
        slot: slot.to_string(),
        item_id: item.item_id,
        ilevel,
        simc_string: item.simc_string.clone(),
        origin: item.origin,
        bonus_ids: item.bonus_ids.clone(),
        enchant_id: item.enchant_id,
        gem_id: item.gem_id,
        name,
        icon,
        quality,
        quality_color: class_data::quality_color(quality).to_string(),
        tag,
        upgrade,
        sockets,
        enchant_name,
        gem_name,
        gem_icon,
        is_catalyst: false,
    }
}

/// Determine eligible slots for an item using the item DB's inventory_type.
/// Falls back to raw_slot + paired slots if no DB info available.
fn eligible_slots(item: &RawParsedItem, spec: &str) -> Vec<String> {
    let info = item_db::get_item_info(item.item_id, Some(&item.bonus_ids));
    if let Some(ref info) = info {
        let inv_type = info
            .get("inventory_type")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if inv_type > 0 {
            return class_data::inv_type_to_slots(inv_type, spec)
                .into_iter()
                .map(|s| s.to_string())
                .collect();
        }
    }
    // Fallback: use raw_slot + paired slot
    let mut slots = vec![item.raw_slot.clone()];
    if let Some(paired) = class_data::paired_slot(&item.raw_slot) {
        slots.push(paired.to_string());
    }
    slots
}

/// Resolve a flat list of parsed items into a slot-organized, enriched gear set.
pub fn resolve_gear(parse_result: &ParseResult) -> ResolveGearResponse {
    resolve_gear_impl(parse_result, None)
}

/// Resolve gear with optional catalyst alternative generation.
/// `catalyst_charges` should be pre-parsed from the raw simc input.
pub fn resolve_gear_with_catalyst(
    parse_result: &ParseResult,
    catalyst_charges: Option<u32>,
) -> ResolveGearResponse {
    resolve_gear_impl(parse_result, catalyst_charges)
}

fn resolve_gear_impl(parse_result: &ParseResult, catalyst_charges: Option<u32>) -> ResolveGearResponse {
    let character = &parse_result.character;
    let spec = character.spec.as_deref().unwrap_or("");
    let class_name = character.class_name.as_deref().unwrap_or("");
    let max_armor = character.max_armor();
    let allowed_weapons = class_data::class_allowed_weapons(class_name);
    let can_dw = character.can_dual_wield();

    let mut slots: HashMap<String, SlotResolution> = HashMap::new();
    let mut excluded: Vec<ExcludedItem> = Vec::new();

    // Track seen dedup keys per slot
    let mut seen_per_slot: HashMap<String, HashSet<String>> = HashMap::new();

    // Separate equipped and non-equipped items
    let equipped_items: Vec<&RawParsedItem> = parse_result
        .items
        .iter()
        .filter(|i| i.origin == ItemOrigin::Equipped)
        .collect();
    let other_items: Vec<&RawParsedItem> = parse_result
        .items
        .iter()
        .filter(|i| i.origin != ItemOrigin::Equipped)
        .collect();

    // Helper to get or create slot resolution
    fn get_slot<'a>(
        slots: &'a mut HashMap<String, SlotResolution>,
        s: &str,
    ) -> &'a mut SlotResolution {
        slots
            .entry(s.to_string())
            .or_insert_with(|| SlotResolution {
                equipped: None,
                alternatives: Vec::new(),
            })
    }

    fn get_seen<'a>(
        seen: &'a mut HashMap<String, HashSet<String>>,
        s: &str,
    ) -> &'a mut HashSet<String> {
        seen.entry(s.to_string()).or_default()
    }

    // Step 1: Place equipped items in their raw_slot
    for item in &equipped_items {
        if item.item_id == 0 {
            continue;
        }
        let slot = &item.raw_slot;
        if !GEAR_SLOTS.contains(&slot.as_str()) {
            continue;
        }

        let dk = dedup_key(item);
        get_seen(&mut seen_per_slot, slot).insert(dk);

        let resolved = enrich(item, slot);
        get_slot(&mut slots, slot).equipped = Some(resolved);
    }

    // Step 2: Dual-wield crossover — add equipped weapons as alternatives in the other hand
    if can_dw {
        let mh_equipped = equipped_items.iter().find(|i| i.raw_slot == "main_hand");
        let oh_equipped = equipped_items.iter().find(|i| i.raw_slot == "off_hand");

        // Main hand → off hand alternative
        if let Some(mh) = mh_equipped {
            if mh.item_id > 0 {
                let info = item_db::get_item_info(mh.item_id, Some(&mh.bonus_ids));
                let inv_type = info
                    .as_ref()
                    .and_then(|i| i.get("inventory_type").and_then(|v| v.as_u64()))
                    .unwrap_or(0);
                // Only one-hand weapons cross over (inv_type 13)
                if inv_type == 13 {
                    let dk = dedup_key(mh);
                    if !get_seen(&mut seen_per_slot, "off_hand").contains(&dk) {
                        get_seen(&mut seen_per_slot, "off_hand").insert(dk);
                        let mut resolved = enrich(mh, "off_hand");
                        resolved.origin = ItemOrigin::Equipped;
                        get_slot(&mut slots, "off_hand").alternatives.push(resolved);
                    }
                }
            }
        }

        // Off hand → main hand alternative
        if let Some(oh) = oh_equipped {
            if oh.item_id > 0 {
                let info = item_db::get_item_info(oh.item_id, Some(&oh.bonus_ids));
                let inv_type = info
                    .as_ref()
                    .and_then(|i| i.get("inventory_type").and_then(|v| v.as_u64()))
                    .unwrap_or(0);
                if inv_type == 13 {
                    let dk = dedup_key(oh);
                    if !get_seen(&mut seen_per_slot, "main_hand").contains(&dk) {
                        get_seen(&mut seen_per_slot, "main_hand").insert(dk);
                        let mut resolved = enrich(oh, "main_hand");
                        resolved.origin = ItemOrigin::Equipped;
                        get_slot(&mut slots, "main_hand")
                            .alternatives
                            .push(resolved);
                    }
                }
            }
        }
    }

    // Step 3: Place non-equipped items (bags + vault) in all eligible slots
    for item in &other_items {
        if item.item_id == 0 {
            continue;
        }

        let item_eligible = eligible_slots(item, spec);
        if item_eligible.is_empty() {
            continue;
        }

        // Armor type check
        let mut armor_excluded = false;
        if let Some(max) = max_armor {
            if let Some(sub) = item_db::get_item_armor_subclass(item.item_id) {
                if sub > 0 && sub > max {
                    armor_excluded = true;
                }
            }
        }

        // Weapon type check
        let mut weapon_excluded = false;
        if let Some(weapons) = allowed_weapons {
            let info = item_db::get_item_info(item.item_id, Some(&item.bonus_ids));
            if let Some(ref info) = info {
                let item_class = info.get("item_class").and_then(|v| v.as_u64()).unwrap_or(0);
                if item_class == 2 {
                    let weapon_sub = info
                        .get("item_subclass")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(999);
                    if !weapons.contains(&weapon_sub) {
                        weapon_excluded = true;
                    }
                }
            }
        }

        for slot in &item_eligible {
            if !GEAR_SLOTS.contains(&slot.as_str()) {
                continue;
            }

            // Only apply armor exclusion to armor slots
            if armor_excluded && ARMOR_SLOTS.contains(&slot.as_str()) {
                excluded.push(ExcludedItem {
                    uid: make_uid(item),
                    item_id: item.item_id,
                    name: item.name.clone(),
                    reason: "Wrong armor type".to_string(),
                });
                continue;
            }

            // Weapon type exclusion for weapon slots
            if weapon_excluded && matches!(slot.as_str(), "main_hand" | "off_hand") {
                excluded.push(ExcludedItem {
                    uid: make_uid(item),
                    item_id: item.item_id,
                    name: item.name.clone(),
                    reason: "Wrong weapon type".to_string(),
                });
                continue;
            }

            let dk = dedup_key(item);
            if get_seen(&mut seen_per_slot, slot).contains(&dk) {
                continue;
            }
            get_seen(&mut seen_per_slot, slot).insert(dk);

            let resolved = enrich(item, slot);
            get_slot(&mut slots, slot).alternatives.push(resolved);
        }
    }

    // Sort alternatives by ilevel descending
    for slot_res in slots.values_mut() {
        slot_res
            .alternatives
            .sort_by(|a, b| b.ilevel.cmp(&a.ilevel));
    }

    // Catalyst pass: generate tier alternatives for non-tier items in tier slots
    if catalyst_charges.is_some() {
        if let Some(class_id) = class_data::class_wow_id(class_name) {
            generate_catalyst_alternatives(&mut slots, class_id);
        }
    }

    ResolveGearResponse {
        character: CharacterResolveInfo {
            class_name: character.class_name.clone(),
            spec: character.spec.clone(),
            can_dual_wield: can_dw,
        },
        base_profile: parse_result.base_profile.clone(),
        slots,
        excluded,
        talent_loadouts: parse_result.talent_loadouts.clone(),
        catalyst_charges: catalyst_charges,
    }
}

/// Tier slots eligible for catalyst conversion.
const TIER_SLOTS: &[&str] = &["head", "shoulder", "chest", "hands", "legs"];

/// Inventory type for each tier slot (used for catalyst tier item lookup).
fn slot_to_inv_type(slot: &str) -> Option<u64> {
    match slot {
        "head" => Some(1),
        "shoulder" => Some(3),
        "chest" => Some(5),
        "hands" => Some(10),
        "legs" => Some(7),
        _ => None,
    }
}

/// Generate catalyzed tier alternatives for non-tier items in tier slots.
/// Creates one catalyst copy per slot using the highest-ilevel non-tier item.
fn generate_catalyst_alternatives(
    slots: &mut HashMap<String, SlotResolution>,
    wow_class_id: u64,
) {
    for &tier_slot in TIER_SLOTS {
        let inv_type = match slot_to_inv_type(tier_slot) {
            Some(t) => t,
            None => continue,
        };
        let tier_info = match item_db::catalyst_tier_item(wow_class_id, inv_type) {
            Some(t) => t,
            None => continue,
        };

        let slot_res = match slots.get(tier_slot) {
            Some(s) => s,
            None => continue,
        };

        // Skip if the equipped item is already this tier piece
        if let Some(ref eq) = slot_res.equipped {
            if eq.item_id == tier_info.item_id || item_db::is_catalyst_tier_item(eq.item_id) {
                continue;
            }
        }

        // Use the equipped item as the catalyst source (highest ilevel in the slot)
        let source = match &slot_res.equipped {
            Some(eq) => eq.clone(),
            None => continue,
        };

        let tier_item_id = tier_info.item_id;

        // Build catalyst bonus_ids: keep only ilevel-related bonuses from the source,
        // then add the tier set marker bonus (13575) for tier set items.
        let mut catalyst_bonus_ids = item_db::filter_ilevel_bonus_ids(&source.bonus_ids);
        // All items from item-conversions with itemSetId are tier set pieces
        catalyst_bonus_ids.push(item_db::tier_set_bonus_id());
        catalyst_bonus_ids.sort();

        // Build simc_string with tier item_id and catalyst bonus_ids
        let bonus_str = catalyst_bonus_ids
            .iter()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join("/");
        let mut simc_parts = vec![format!(",id={}", tier_item_id)];
        if !bonus_str.is_empty() {
            simc_parts.push(format!(",bonus_id={}", bonus_str));
        }
        if source.enchant_id > 0 {
            simc_parts.push(format!(",enchant_id={}", source.enchant_id));
        }
        if source.gem_id > 0 {
            simc_parts.push(format!(",gem_id={}", source.gem_id));
        }
        let new_simc = simc_parts.join("");

        // Enrich from the tier item with catalyst bonus_ids
        let tier_db_info = item_db::get_item_info(tier_item_id, Some(&catalyst_bonus_ids));
        let (name, icon, quality, tag, upgrade) = if let Some(ref info) = tier_db_info {
            (
                info.get("name").and_then(|n| n.as_str()).unwrap_or(&tier_info.name).to_string(),
                info.get("icon").and_then(|i| i.as_str()).unwrap_or(&tier_info.icon).to_string(),
                info.get("quality").and_then(|q| q.as_u64()).unwrap_or(4),
                info.get("tag").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                info.get("upgrade").and_then(|u| u.as_str()).unwrap_or("").to_string(),
            )
        } else {
            (tier_info.name.clone(), tier_info.icon.clone(), 4, String::new(), String::new())
        };

        // Use the source item's ilevel — it's already correctly resolved
        let ilevel = source.ilevel;

        // UID must match the format used by profileset_generator::make_item_uid:
        // "item_id:sorted_bonus_ids:origin:slot"
        let bonus_key = catalyst_bonus_ids
            .iter()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(":");
        let uid = format!("{}:{}:{}:{}", tier_item_id, bonus_key, source.origin.as_str(), tier_slot);

        let catalyst_item = ResolvedItem {
            uid,
            slot: tier_slot.to_string(),
            item_id: tier_item_id,
            ilevel,
            simc_string: new_simc,
            origin: source.origin,
            bonus_ids: catalyst_bonus_ids,
            enchant_id: source.enchant_id,
            gem_id: source.gem_id,
            name,
            icon,
            quality,
            quality_color: class_data::quality_color(quality).to_string(),
            tag,
            upgrade,
            sockets: 0,
            enchant_name: source.enchant_name.clone(),
            gem_name: source.gem_name.clone(),
            gem_icon: source.gem_icon.clone(),
            is_catalyst: true,
        };

        slots.get_mut(tier_slot).unwrap().alternatives.push(catalyst_item);
    }
}
