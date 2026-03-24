use once_cell::sync::OnceCell;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

static ITEMS: OnceCell<HashMap<u64, Value>> = OnceCell::new();
static ENCHANTS: OnceCell<HashMap<u64, Value>> = OnceCell::new();
static ENCHANTS_BY_ITEM_ID: OnceCell<HashMap<u64, Value>> = OnceCell::new();
static BONUSES: OnceCell<HashMap<u64, Value>> = OnceCell::new();
static UPGRADE_MAX: OnceCell<HashMap<u64, u64>> = OnceCell::new();
static INSTANCES: OnceCell<Vec<Value>> = OnceCell::new();
static DROPS_BY_ENCOUNTER: OnceCell<HashMap<i64, Vec<Value>>> = OnceCell::new();
// Maps (track_name, level, max) -> {ilvl, bonus_id, quality} for upgrade tracks
static UPGRADE_TRACKS: OnceCell<HashMap<(String, u64, u64), (u64, u64, u64)>> = OnceCell::new();
static SEASON_CONFIG: OnceCell<Value> = OnceCell::new();

pub fn load(data_dir: &Path) {
    // equippable-items-full.json — array of {id, name, icon, quality, itemLevel, ...}
    let items_path = data_dir.join("equippable-items-full.json");
    if items_path.exists() {
        let data: Vec<Value> = serde_json::from_reader(
            std::io::BufReader::new(fs::File::open(&items_path).unwrap()),
        )
        .unwrap_or_default();
        let map: HashMap<u64, Value> = data
            .into_iter()
            .filter_map(|v| {
                let id = v.get("id")?.as_u64()?;
                Some((id, v))
            })
            .collect();
        println!("Loaded {} items", map.len());
        let _ = ITEMS.set(map);
    }

    // enchantments.json — array of {id, displayName, ...}
    let enchants_path = data_dir.join("enchantments.json");
    if enchants_path.exists() {
        let data: Vec<Value> = serde_json::from_reader(
            std::io::BufReader::new(fs::File::open(&enchants_path).unwrap()),
        )
        .unwrap_or_default();
        let by_id: HashMap<u64, Value> = data
            .iter()
            .filter_map(|v| {
                let id = v.get("id")?.as_u64()?;
                Some((id, v.clone()))
            })
            .collect();
        let by_item_id: HashMap<u64, Value> = data
            .into_iter()
            .filter_map(|v| {
                let item_id = v.get("itemId")?.as_u64()?;
                Some((item_id, v))
            })
            .collect();
        println!("Loaded {} enchants", by_id.len());
        let _ = ENCHANTS.set(by_id);
        let _ = ENCHANTS_BY_ITEM_ID.set(by_item_id);
    }

    // bonuses.json — object keyed by bonus ID string
    let bonuses_path = data_dir.join("bonuses.json");
    if bonuses_path.exists() {
        let raw: HashMap<String, Value> = serde_json::from_reader(
            std::io::BufReader::new(fs::File::open(&bonuses_path).unwrap()),
        )
        .unwrap_or_default();
        let map: HashMap<u64, Value> = raw
            .into_iter()
            .filter_map(|(k, v)| {
                let id = k.parse::<u64>().ok()?;
                Some((id, v))
            })
            .collect();

        // Build upgrade group index
        let mut groups: HashMap<u64, Vec<(u64, u64)>> = HashMap::new(); // group -> [(bonus_id, level)]
        for (bid, bonus) in &map {
            if let Some(upgrade) = bonus.get("upgrade") {
                if let (Some(group), Some(level)) =
                    (upgrade.get("group").and_then(|g| g.as_u64()),
                     upgrade.get("level").and_then(|l| l.as_u64()))
                {
                    groups.entry(group).or_default().push((*bid, level));
                }
            }
        }
        let mut upgrade_max: HashMap<u64, u64> = HashMap::new();
        for members in groups.values() {
            let max_bonus_id = members
                .iter()
                .max_by_key(|(_, level)| *level)
                .map(|(id, _)| *id)
                .unwrap_or(0);
            for (bid, _) in members {
                upgrade_max.insert(*bid, max_bonus_id);
            }
        }
        println!("Loaded {} bonuses, {} upgrade groups", map.len(), groups.len());
        let _ = BONUSES.set(map);
        let _ = UPGRADE_MAX.set(upgrade_max);
    }

    // bonus-upgrade-sets.json + seasons.json -> upgrade track lookup
    let bus_path = data_dir.join("bonus-upgrade-sets.json");
    let seasons_path = data_dir.join("seasons.json");
    if bus_path.exists() {
        let bus_raw: HashMap<String, Vec<Value>> = serde_json::from_reader(
            std::io::BufReader::new(fs::File::open(&bus_path).unwrap()),
        ).unwrap_or_default();

        // Find active season's groups
        let mut active_groups: Option<Vec<u64>> = None;
        if seasons_path.exists() {
            let seasons: Vec<Value> = serde_json::from_reader(
                std::io::BufReader::new(fs::File::open(&seasons_path).unwrap()),
            ).unwrap_or_default();
            if let Some(active) = seasons.iter().find(|s| s.get("active").and_then(|a| a.as_bool()).unwrap_or(false)) {
                let groups: Vec<u64> = active.get("bonusListGroups")
                    .and_then(|g| g.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
                    .unwrap_or_default();
                let name = active.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                println!("Active season: {}, groups: {:?}", name, groups);
                active_groups = Some(groups);
            }
        }

        let bonuses_map = BONUSES.get();
        let mut tracks: HashMap<(String, u64, u64), (u64, u64, u64)> = HashMap::new();
        for (group_id_str, entries) in &bus_raw {
            let group_id: u64 = group_id_str.parse().unwrap_or(0);
            if let Some(ref ag) = active_groups {
                if !ag.contains(&group_id) { continue; }
            }
            for entry in entries {
                let name = entry.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let level = entry.get("level").and_then(|l| l.as_u64()).unwrap_or(0);
                let max_level = entry.get("max").and_then(|m| m.as_u64()).unwrap_or(0);
                let ilvl = entry.get("itemLevel").and_then(|i| i.as_u64()).unwrap_or(0);
                let bonus_id = entry.get("bonusId").and_then(|b| b.as_u64()).unwrap_or(0);
                // Quality from bonuses.json
                let quality = bonuses_map
                    .and_then(|bm| bm.get(&bonus_id))
                    .and_then(|b| b.get("quality"))
                    .and_then(|q| q.as_u64())
                    .unwrap_or(4);
                if !name.is_empty() && level > 0 && max_level > 0 && ilvl > 0 {
                    tracks.insert((name.to_string(), level, max_level), (ilvl, bonus_id, quality));
                }
            }
        }
        println!("Indexed {} upgrade track entries", tracks.len());
        let _ = UPGRADE_TRACKS.set(tracks);
    }

    // instances.json — array of raid/dungeon definitions
    let instances_path = data_dir.join("instances.json");
    if instances_path.exists() {
        let data: Vec<Value> = serde_json::from_reader(
            std::io::BufReader::new(fs::File::open(&instances_path).unwrap()),
        )
        .unwrap_or_default();
        println!("Loaded {} instances", data.len());
        let _ = INSTANCES.set(data);
    }

    // Build encounter -> items index
    let mut drops: HashMap<i64, Vec<Value>> = HashMap::new();
    if let Some(items_map) = ITEMS.get() {
        for item in items_map.values() {
            if let Some(sources) = item.get("sources").and_then(|s| s.as_array()) {
                for src in sources {
                    if let Some(eid) = src.get("encounterId").and_then(|e| e.as_i64()) {
                        drops.entry(eid).or_default().push(item.clone());
                    }
                }
            }
        }
    }
    println!("Indexed drops for {} encounters", drops.len());
    let _ = DROPS_BY_ENCOUNTER.set(drops);

    // season-config.json
    let season_path = data_dir.join("season-config.json");
    if season_path.exists() {
        let cfg: Value = serde_json::from_reader(
            std::io::BufReader::new(fs::File::open(&season_path).unwrap()),
        )
        .unwrap_or(Value::Null);
        let name = cfg.get("season").and_then(|s| s.as_str()).unwrap_or("unknown");
        println!("Loaded season config: {}", name);
        let _ = SEASON_CONFIG.set(cfg);
    }
}

fn items() -> &'static HashMap<u64, Value> {
    ITEMS.get().expect("Game data not loaded")
}

fn enchants() -> &'static HashMap<u64, Value> {
    ENCHANTS.get().expect("Game data not loaded")
}

fn enchants_by_item_id() -> &'static HashMap<u64, Value> {
    ENCHANTS_BY_ITEM_ID.get().expect("Game data not loaded")
}

fn bonuses() -> &'static HashMap<u64, Value> {
    BONUSES.get().expect("Game data not loaded")
}

fn upgrade_max() -> &'static HashMap<u64, u64> {
    UPGRADE_MAX.get().expect("Game data not loaded")
}

pub const QUALITY_NAMES: &[(u64, &str)] = &[
    (0, "poor"),
    (1, "common"),
    (2, "uncommon"),
    (3, "rare"),
    (4, "epic"),
    (5, "legendary"),
    (6, "artifact"),
    (7, "heirloom"),
];

pub fn quality_name(quality: u64) -> &'static str {
    QUALITY_NAMES
        .iter()
        .find(|(q, _)| *q == quality)
        .map(|(_, name)| *name)
        .unwrap_or("common")
}

fn resolve_bonuses(bonus_ids: &[u64]) -> Value {
    let mut result = serde_json::json!({});
    for bid in bonus_ids {
        if let Some(bonus) = bonuses().get(bid) {
            if let Some(q) = bonus.get("quality") {
                result["quality"] = q.clone();
            }
            if let Some(il) = bonus.get("itemLevel").and_then(|il| il.get("amount")) {
                result["ilevel"] = il.clone();
            }
            if let Some(tag) = bonus.get("tag") {
                result["tag"] = tag.clone();
            }
            if let Some(socket) = bonus.get("socket") {
                result["sockets"] = socket.clone();
            }
            if let Some(upgrade) = bonus.get("upgrade").and_then(|u| u.get("fullName")) {
                result["upgrade"] = upgrade.clone();
            }
        }
    }
    result
}

pub fn get_item_info(item_id: u64, bonus_ids: Option<&[u64]>) -> Option<Value> {
    let item = items().get(&item_id)?;

    let mut quality = item.get("quality").and_then(|q| q.as_u64()).unwrap_or(1);
    let mut ilevel = item.get("itemLevel").and_then(|i| i.as_u64()).unwrap_or(0);
    let mut tag = String::new();
    let mut sockets: u64 = 0;
    let mut upgrade = String::new();

    if let Some(bids) = bonus_ids {
        let resolved = resolve_bonuses(bids);
        if let Some(q) = resolved.get("quality").and_then(|q| q.as_u64()) {
            quality = q;
        }
        if let Some(i) = resolved.get("ilevel").and_then(|i| i.as_u64()) {
            ilevel = i;
        }
        if let Some(t) = resolved.get("tag").and_then(|t| t.as_str()) {
            tag = t.to_string();
        }
        if let Some(s) = resolved.get("sockets").and_then(|s| s.as_u64()) {
            sockets = s;
        }
        if let Some(u) = resolved.get("upgrade").and_then(|u| u.as_str()) {
            upgrade = u.to_string();
        }
    }

    let armor_subclass = if item.get("itemClass").and_then(|c| c.as_u64()) == Some(4) {
        item.get("itemSubClass").and_then(|s| s.as_u64()).unwrap_or(0)
    } else {
        0
    };

    let inventory_type = item.get("inventoryType").and_then(|v| v.as_u64()).unwrap_or(0);

    Some(serde_json::json!({
        "item_id": item_id,
        "name": item.get("name").and_then(|n| n.as_str()).unwrap_or("Unknown"),
        "quality": quality,
        "quality_name": quality_name(quality),
        "icon": item.get("icon").and_then(|i| i.as_str()).unwrap_or("inv_misc_questionmark"),
        "ilevel": ilevel,
        "tag": tag,
        "sockets": sockets,
        "upgrade": upgrade,
        "armor_subclass": armor_subclass,
        "inventory_type": inventory_type,
    }))
}

pub fn get_enchant_info(enchant_id: u64) -> Option<Value> {
    let enchant = enchants().get(&enchant_id)?;
    let name = enchant
        .get("itemName")
        .or_else(|| enchant.get("displayName"))
        .and_then(|n| n.as_str())
        .unwrap_or("");
    Some(serde_json::json!({
        "enchant_id": enchant_id,
        "name": name,
    }))
}

pub fn get_gem_info(gem_item_id: u64) -> Option<Value> {
    let gem = enchants_by_item_id().get(&gem_item_id)?;
    let name = gem
        .get("itemName")
        .or_else(|| gem.get("displayName"))
        .and_then(|n| n.as_str())
        .unwrap_or("");
    let icon = gem
        .get("itemIcon")
        .or_else(|| gem.get("spellIcon"))
        .and_then(|i| i.as_str())
        .unwrap_or("");
    let quality = gem.get("quality").and_then(|q| q.as_u64()).unwrap_or(3);
    Some(serde_json::json!({
        "gem_id": gem_item_id,
        "name": name,
        "icon": icon,
        "quality": quality,
    }))
}

/// Returns the item's armor subclass: 0=Misc, 1=Cloth, 2=Leather, 3=Mail, 4=Plate.
/// Returns None if the item is not found or is not armor (itemClass != 4).
pub fn get_item_armor_subclass(item_id: u64) -> Option<u64> {
    let item = items().get(&item_id)?;
    let item_class = item.get("itemClass")?.as_u64()?;
    if item_class != 4 {
        return None; // Not armor
    }
    item.get("itemSubClass")?.as_u64()
}

pub fn get_upgrade_options(bonus_ids: &[u64]) -> Option<Vec<Value>> {
    let um = upgrade_max();
    for bid in bonus_ids {
        if um.contains_key(bid) {
            let bonus = bonuses().get(bid)?;
            let group_id = bonus.get("upgrade")?.get("group")?.as_u64()?;
            let mut members: Vec<&Value> = bonuses()
                .values()
                .filter(|b| {
                    b.get("upgrade")
                        .and_then(|u| u.get("group"))
                        .and_then(|g| g.as_u64())
                        == Some(group_id)
                })
                .collect();
            members.sort_by_key(|b| {
                b.get("upgrade")
                    .and_then(|u| u.get("level"))
                    .and_then(|l| l.as_u64())
                    .unwrap_or(0)
            });
            return Some(
                members
                    .into_iter()
                    .filter_map(|b| {
                        let u = b.get("upgrade")?;
                        Some(serde_json::json!({
                            "bonus_id": b.get("id")?.as_u64()?,
                            "level": u.get("level")?.as_u64()?,
                            "max": u.get("max")?.as_u64()?,
                            "name": u.get("name")?.as_str()?,
                            "fullName": u.get("fullName")?.as_str()?,
                            "itemLevel": u.get("itemLevel")?.as_u64()?,
                        }))
                    })
                    .collect(),
            );
        }
    }
    None
}

pub fn upgrade_bonus_ids_to_max(bonus_ids: &[u64]) -> Vec<u64> {
    let um = upgrade_max();
    bonus_ids
        .iter()
        .map(|bid| *um.get(bid).unwrap_or(bid))
        .collect()
}

pub fn upgrade_simc_input(simc_input: &str) -> String {
    let re = regex::Regex::new(r"bonus_id=([0-9/:]+)").unwrap();
    re.replace_all(simc_input, |caps: &regex::Captures| {
        let raw = &caps[1];
        let sep = if raw.contains('/') { "/" } else { ":" };
        let ids: Vec<u64> = raw
            .split(&['/', ':'][..])
            .filter_map(|s| s.parse().ok())
            .collect();
        let upgraded = upgrade_bonus_ids_to_max(&ids);
        format!(
            "bonus_id={}",
            upgraded
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(sep)
        )
    })
    .to_string()
}

pub fn upgrade_items_by_slot(
    items_by_slot: &HashMap<String, Vec<Value>>,
) -> HashMap<String, Vec<Value>> {
    let bonus_re = regex::Regex::new(r"bonus_id=([0-9/:]+)").unwrap();
    let mut result = HashMap::new();

    for (slot, items) in items_by_slot {
        let new_items: Vec<Value> = items
            .iter()
            .map(|item| {
                let old_bonus_ids: Vec<u64> = item
                    .get("bonus_ids")
                    .and_then(|b| b.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
                    .unwrap_or_default();

                let new_bonus_ids = upgrade_bonus_ids_to_max(&old_bonus_ids);

                if new_bonus_ids == old_bonus_ids {
                    return item.clone();
                }

                let mut updated = item.clone();
                updated["bonus_ids"] = serde_json::json!(new_bonus_ids);

                // Update simc_string with new bonus_ids
                if let Some(simc) = item.get("simc_string").and_then(|s| s.as_str()) {
                    let new_simc = bonus_re.replace(simc, |caps: &regex::Captures| {
                        let raw = &caps[1];
                        let sep = if raw.contains('/') { "/" } else { ":" };
                        format!(
                            "bonus_id={}",
                            new_bonus_ids
                                .iter()
                                .map(|id| id.to_string())
                                .collect::<Vec<_>>()
                                .join(sep)
                        )
                    }).to_string();
                    updated["simc_string"] = serde_json::json!(new_simc);
                }

                // Recalculate ilevel from base item + new bonuses
                let item_id = item.get("item_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let items_map = self::items();
                if let Some(base_item) = items_map.get(&item_id) {
                    let base_ilevel = base_item.get("itemLevel").and_then(|v: &Value| v.as_u64()).unwrap_or(0);
                    let resolved = resolve_bonuses(&new_bonus_ids);
                    let new_ilevel = resolved.get("ilevel").and_then(|v: &Value| v.as_u64()).unwrap_or(base_ilevel);
                    updated["ilevel"] = serde_json::json!(new_ilevel);
                }

                updated
            })
            .collect();
        result.insert(slot.clone(), new_items);
    }
    result
}

pub fn apply_copy_enchants(
    items_by_slot: &HashMap<String, Vec<Value>>,
) -> HashMap<String, Vec<Value>> {
    let re = regex::Regex::new(r"enchant_id=\d+").unwrap();
    let id_re = regex::Regex::new(r"(,id=\d+)").unwrap();
    let mut result = HashMap::new();

    for (slot, items) in items_by_slot {
        let equipped = items.iter().find(|it| {
            it.get("is_equipped")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        });
        let ench_id = equipped
            .and_then(|e| e.get("enchant_id"))
            .and_then(|e| e.as_u64())
            .unwrap_or(0);

        if ench_id == 0 {
            result.insert(slot.clone(), items.clone());
            continue;
        }

        let new_items: Vec<Value> = items
            .iter()
            .map(|item| {
                let is_equipped = item
                    .get("is_equipped")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let current_ench = item
                    .get("enchant_id")
                    .and_then(|e| e.as_u64())
                    .unwrap_or(0);

                if is_equipped || current_ench == ench_id {
                    return item.clone();
                }

                let mut updated = item.clone();
                updated["enchant_id"] = serde_json::json!(ench_id);

                if let Some(simc) = item.get("simc_string").and_then(|s| s.as_str()) {
                    let new_simc = if re.is_match(simc) {
                        re.replace(simc, &format!("enchant_id={}", ench_id))
                            .to_string()
                    } else {
                        id_re
                            .replace(simc, &format!("$1,enchant_id={}", ench_id))
                            .to_string()
                    };
                    updated["simc_string"] = serde_json::json!(new_simc);
                }

                updated
            })
            .collect();
        result.insert(slot.clone(), new_items);
    }
    result
}

pub fn get_instances() -> &'static Vec<Value> {
    INSTANCES.get().expect("Game data not loaded")
}

/// Returns all upgrade tracks grouped by track name.
/// Format: { "Hero": [ { level, max_level, ilvl, bonus_id, quality }, ... ], ... }
pub fn get_upgrade_tracks() -> Value {
    let mut result: HashMap<String, Vec<Value>> = HashMap::new();
    if let Some(tracks) = UPGRADE_TRACKS.get() {
        for ((name, level, max_level), (ilvl, bonus_id, quality)) in tracks {
            result.entry(name.clone()).or_default().push(serde_json::json!({
                "level": level,
                "max_level": max_level,
                "ilvl": ilvl,
                "bonus_id": bonus_id,
                "quality": quality,
            }));
        }
        // Sort each track's levels
        for levels in result.values_mut() {
            levels.sort_by_key(|v| v.get("level").and_then(|l| l.as_u64()).unwrap_or(0));
        }
    }
    serde_json::json!(result)
}

fn inventory_type_slot(inv_type: u64) -> &'static str {
    match inv_type {
        1 => "Head", 2 => "Neck", 3 => "Shoulder", 4 => "Shirt",
        5 | 20 => "Chest", 6 => "Waist", 7 => "Legs", 8 => "Feet",
        9 => "Wrist", 10 => "Hands", 11 => "Finger", 12 => "Trinket",
        13 => "One-Hand", 14 => "Shield", 15 | 26 => "Ranged",
        16 => "Back", 17 => "Two-Hand", 19 => "Tabard",
        21 => "Main Hand", 22 => "Off Hand", 23 => "Held In Off-Hand",
        _ => "Other",
    }
}

const SLOT_ORDER: &[&str] = &[
    "Head", "Neck", "Shoulder", "Back", "Chest", "Wrist", "Hands",
    "Waist", "Legs", "Feet", "Finger", "Trinket",
    "One-Hand", "Main Hand", "Off Hand", "Two-Hand",
    "Held In Off-Hand", "Shield", "Ranged",
];

fn class_allowed_weapons(class_name: &str) -> Option<&'static [u64]> {
    match class_name {
        "warrior" => Some(&[0, 1, 4, 5, 6, 7, 8, 13, 15]),
        "paladin" => Some(&[0, 1, 4, 5, 6, 7, 8]),
        "hunter" => Some(&[2, 3, 6, 18]),
        "rogue" => Some(&[0, 4, 7, 13, 15]),
        "priest" => Some(&[4, 10, 15, 19]),
        "death_knight" | "deathknight" => Some(&[0, 1, 4, 5, 7, 8]),
        "shaman" => Some(&[0, 1, 4, 5, 10, 13]),
        "mage" => Some(&[7, 10, 15, 19]),
        "warlock" => Some(&[7, 10, 15, 19]),
        "monk" => Some(&[0, 4, 6, 7, 10, 13]),
        "druid" => Some(&[4, 5, 6, 10, 13, 15]),
        "demon_hunter" | "demonhunter" => Some(&[0, 7, 9, 13]),
        "evoker" => Some(&[0, 4, 7, 10, 13, 15]),
        _ => None,
    }
}

fn class_max_armor(class_name: &str) -> Option<u64> {
    match class_name {
        "priest" | "mage" | "warlock" => Some(1),
        "rogue" | "monk" | "druid" | "demon_hunter" | "demonhunter" => Some(2),
        "hunter" | "shaman" | "evoker" => Some(3),
        "warrior" | "paladin" | "death_knight" | "deathknight" => Some(4),
        _ => None,
    }
}

fn class_spec_ids(class_name: &str, spec_name: Option<&str>) -> Vec<u64> {
    let all: &[(&str, u64)] = match class_name {
        "warrior" => &[("arms", 71), ("fury", 72), ("protection", 73)],
        "paladin" => &[("holy", 65), ("protection", 66), ("retribution", 70)],
        "hunter" => &[("beast_mastery", 253), ("marksmanship", 254), ("survival", 255)],
        "rogue" => &[("assassination", 259), ("outlaw", 260), ("subtlety", 261)],
        "priest" => &[("discipline", 256), ("holy", 257), ("shadow", 258)],
        "death_knight" | "deathknight" => &[("blood", 250), ("frost", 251), ("unholy", 252)],
        "shaman" => &[("elemental", 262), ("enhancement", 263), ("restoration", 264)],
        "mage" => &[("arcane", 62), ("fire", 63), ("frost", 64)],
        "warlock" => &[("affliction", 265), ("demonology", 266), ("destruction", 267)],
        "monk" => &[("brewmaster", 268), ("mistweaver", 270), ("windwalker", 269)],
        "druid" => &[("balance", 102), ("feral", 103), ("guardian", 104), ("restoration", 105)],
        "demon_hunter" | "demonhunter" => &[("havoc", 577), ("vengeance", 581)],
        "evoker" => &[("devastation", 1467), ("preservation", 1468), ("augmentation", 1473)],
        _ => &[],
    };
    if let Some(spec) = spec_name {
        all.iter().filter(|(n, _)| *n == spec).map(|(_, id)| *id).collect()
    } else {
        all.iter().map(|(_, id)| *id).collect()
    }
}

// Armor inventory types where subclass filtering applies
const ARMOR_SLOT_TYPES: &[u64] = &[1, 3, 5, 6, 7, 8, 9, 10, 20];

// Dungeon difficulty -> (key, track_name, level) for end-of-dungeon drops. Uses max=6 tracks.
fn dungeon_normal_ilvl() -> u64 {
    season_cfg()
        .get("dungeonNormal")
        .and_then(|d| d.get("ilvl"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

fn dungeon_normal_quality() -> u64 {
    season_cfg()
        .get("dungeonNormal")
        .and_then(|d| d.get("quality"))
        .and_then(|v| v.as_u64())
        .unwrap_or(3)
}

static EMPTY_SEASON_CONFIG: once_cell::sync::Lazy<Value> = once_cell::sync::Lazy::new(|| serde_json::json!({}));

fn season_cfg() -> &'static Value {
    SEASON_CONFIG.get().unwrap_or(&EMPTY_SEASON_CONFIG)
}

fn upgrade_track_max() -> u64 {
    // Derive from loaded tracks — find the most common max value
    if let Some(tracks) = UPGRADE_TRACKS.get() {
        let mut counts: HashMap<u64, usize> = HashMap::new();
        for (_, _, max) in tracks.keys() {
            *counts.entry(*max).or_default() += 1;
        }
        counts.into_iter().max_by_key(|(_, count)| *count).map(|(max, _)| max).unwrap_or(6)
    } else {
        6
    }
}

fn difficulty_track_name(difficulty: &str) -> Option<String> {
    season_cfg()
        .get("raidDifficultyTracks")
        .and_then(|m| m.get(difficulty))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn encounter_upgrade_level(encounter_id: i64) -> Option<u64> {
    season_cfg()
        .get("encounterUpgradeLevel")
        .and_then(|m| m.get(&encounter_id.to_string()))
        .and_then(|v| v.as_u64())
}

pub fn get_instance_drops(
    instance_id: i64,
    class_name: Option<&str>,
    spec_name: Option<&str>,
) -> Option<serde_json::Map<String, Value>> {
    let instances = get_instances();
    let instance = instances.iter().find(|i| {
        i.get("id").and_then(|id| id.as_i64()) == Some(instance_id)
    })?;

    let max_armor = class_name.and_then(class_max_armor);
    let allowed_weapons = class_name.and_then(class_allowed_weapons);
    let allowed_specs: Vec<u64> = class_name
        .map(|c| class_spec_ids(c, spec_name))
        .unwrap_or_default();

    let encounters = instance.get("encounters")?.as_array()?;
    let encounter_ids: HashMap<i64, String> = encounters
        .iter()
        .filter_map(|e| {
            let id = e.get("id")?.as_i64()?;
            let name = e.get("name")?.as_str()?.to_string();
            Some((id, name))
        })
        .collect();

    let drops_map = DROPS_BY_ENCOUNTER.get().expect("Game data not loaded");
    let mut by_slot: HashMap<&str, Vec<Value>> = HashMap::new();
    let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();

    for eid in encounter_ids.keys() {
        if let Some(items_list) = drops_map.get(eid) {
            for item in items_list {
                let item_id = item.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                if !seen.insert(item_id) {
                    continue;
                }

                let inv_type = item.get("inventoryType").and_then(|v| v.as_u64()).unwrap_or(0);

                // Filter by armor type
                if let Some(max) = max_armor {
                    if ARMOR_SLOT_TYPES.contains(&inv_type)
                        && item.get("itemClass").and_then(|c| c.as_u64()) == Some(4)
                    {
                        let sub = item.get("itemSubClass").and_then(|s| s.as_u64()).unwrap_or(0);
                        if sub != 0 && sub != max {
                            continue;
                        }
                    }
                }

                // Filter by weapon type
                if let Some(weapons) = allowed_weapons {
                    if item.get("itemClass").and_then(|c| c.as_u64()) == Some(2) {
                        let weapon_sub = item.get("itemSubClass").and_then(|s| s.as_u64()).unwrap_or(999);
                        if !weapons.contains(&weapon_sub) {
                            continue;
                        }
                    }
                }

                // Filter shields — only warriors, paladins, shamans can equip them
                if inv_type == 14 {
                    if let Some(cn) = class_name {
                        if !matches!(cn, "warrior" | "paladin" | "shaman") {
                            continue;
                        }
                    }
                }

                // Filter off-hand items — casters only (priests, mages, warlocks, druids, evokers)
                if inv_type == 23 {
                    if let Some(cn) = class_name {
                        if !matches!(cn, "priest" | "mage" | "warlock" | "druid" | "shaman" | "evoker") {
                            continue;
                        }
                    }
                }

                // Filter spec restrictions
                if let Some(specs) = item.get("specs").and_then(|s| s.as_array()) {
                    if !allowed_specs.is_empty() {
                        let item_specs: Vec<u64> = specs.iter().filter_map(|v| v.as_u64()).collect();
                        if !allowed_specs.iter().any(|s| item_specs.contains(s)) {
                            continue;
                        }
                    }
                }

                let slot = inventory_type_slot(inv_type);

                // Compute per-difficulty info from upgrade tracks (raids)
                let upgrade_lvl = encounter_upgrade_level(*eid);
                let track_map = UPGRADE_TRACKS.get();
                let tm = upgrade_track_max();
                let mut diff_info = serde_json::Map::new();
                if let (Some(lvl), Some(tracks)) = (upgrade_lvl, track_map) {
                    for diff in &["lfr", "normal", "heroic", "mythic"] {
                        if let Some(track) = difficulty_track_name(diff) {
                            if let Some(&(ilvl, bonus_id, quality)) = tracks.get(&(track.clone(), lvl, tm)) {
                                diff_info.insert(diff.to_string(), serde_json::json!({
                                    "ilvl": ilvl, "bonus_id": bonus_id, "quality": quality,
                                    "track": track, "level": lvl, "max_level": tm,
                                }));
                            }
                        }
                    }
                }

                // Compute per-difficulty info for dungeons/M+
                let mut dungeon_info = serde_json::Map::new();
                if upgrade_lvl.is_none() {
                    dungeon_info.insert("normal".to_string(), serde_json::json!({
                        "ilvl": dungeon_normal_ilvl(), "bonus_id": 0, "quality": dungeon_normal_quality(),
                    }));
                    if let Some(tracks) = track_map {
                        if let Some(ddt) = season_cfg().get("dungeonDifficultyTracks").and_then(|v| v.as_object()) {
                            for (diff_key, entry) in ddt {
                                let track = entry.get("track").and_then(|v| v.as_str()).unwrap_or("");
                                let level = entry.get("level").and_then(|v| v.as_u64()).unwrap_or(0);
                                if let Some(&(ilvl, bonus_id, quality)) = tracks.get(&(track.to_string(), level, tm)) {
                                    dungeon_info.insert(diff_key.clone(), serde_json::json!({
                                        "ilvl": ilvl, "bonus_id": bonus_id, "quality": quality,
                                        "track": track, "level": level, "max_level": tm,
                                    }));
                                }
                            }
                        }
                    }
                }

                let mut item_json = serde_json::json!({
                    "item_id": item_id,
                    "name": item.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                    "icon": item.get("icon").and_then(|i| i.as_str()).unwrap_or("inv_misc_questionmark"),
                    "quality": item.get("quality").and_then(|q| q.as_u64()).unwrap_or(1),
                    "ilevel": item.get("itemLevel").and_then(|i| i.as_u64()).unwrap_or(0),
                    "inventory_type": inv_type,
                    "encounter": encounter_ids.get(eid).cloned().unwrap_or_default(),
                });
                if !diff_info.is_empty() {
                    item_json["difficulty_info"] = Value::Object(diff_info);
                }
                if !dungeon_info.is_empty() {
                    item_json["dungeon_info"] = Value::Object(dungeon_info);
                }
                by_slot.entry(slot).or_default().push(item_json);
            }
        }
    }

    let mut ordered = serde_json::Map::new();
    for &slot in SLOT_ORDER {
        if let Some(mut slot_items) = by_slot.remove(slot) {
            slot_items.sort_by(|a, b| {
                let ia = b.get("ilevel").and_then(|v| v.as_u64()).unwrap_or(0);
                let ib = a.get("ilevel").and_then(|v| v.as_u64()).unwrap_or(0);
                ia.cmp(&ib)
            });
            ordered.insert(slot.to_string(), Value::Array(slot_items));
        }
    }
    for (slot, mut slot_items) in by_slot {
        slot_items.sort_by(|a, b| {
            let ia = b.get("ilevel").and_then(|v| v.as_u64()).unwrap_or(0);
            let ib = a.get("ilevel").and_then(|v| v.as_u64()).unwrap_or(0);
            ia.cmp(&ib)
        });
        ordered.insert(slot.to_string(), Value::Array(slot_items));
    }

    if ordered.is_empty() { None } else { Some(ordered) }
}

pub fn get_drops_by_type(
    instance_type: &str,
    class_name: Option<&str>,
    spec_name: Option<&str>,
) -> Option<serde_json::Map<String, Value>> {
    let instances = get_instances();
    let mut merged: HashMap<&str, Vec<Value>> = HashMap::new();
    let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();

    for inst in instances {
        let itype = inst.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if itype != instance_type {
            continue;
        }
        let inst_id = inst.get("id").and_then(|id| id.as_i64()).unwrap_or(0);
        if let Some(drops) = get_instance_drops(inst_id, class_name, spec_name) {
            for (slot, items) in &drops {
                if let Some(arr) = items.as_array() {
                    for item in arr {
                        let item_id = item.get("item_id").and_then(|v| v.as_u64()).unwrap_or(0);
                        if seen.insert(item_id) {
                            let slot_str = match slot.as_str() {
                                "Head" => "Head", "Neck" => "Neck", "Shoulder" => "Shoulder",
                                "Back" => "Back", "Chest" => "Chest", "Wrist" => "Wrist",
                                "Hands" => "Hands", "Waist" => "Waist", "Legs" => "Legs",
                                "Feet" => "Feet", "Finger" => "Finger", "Trinket" => "Trinket",
                                "One-Hand" => "One-Hand", "Main Hand" => "Main Hand",
                                "Off Hand" => "Off Hand", "Two-Hand" => "Two-Hand",
                                "Held In Off-Hand" => "Held In Off-Hand", "Shield" => "Shield",
                                "Ranged" => "Ranged", _ => "Other",
                            };
                            merged.entry(slot_str).or_default().push(item.clone());
                        }
                    }
                }
            }
        }
    }

    let mut ordered = serde_json::Map::new();
    for &slot in SLOT_ORDER {
        if let Some(mut slot_items) = merged.remove(slot) {
            slot_items.sort_by(|a, b| {
                let ia = b.get("ilevel").and_then(|v| v.as_u64()).unwrap_or(0);
                let ib = a.get("ilevel").and_then(|v| v.as_u64()).unwrap_or(0);
                ia.cmp(&ib)
            });
            ordered.insert(slot.to_string(), Value::Array(slot_items));
        }
    }

    if ordered.is_empty() { None } else { Some(ordered) }
}
