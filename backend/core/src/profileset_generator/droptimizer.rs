use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;

use super::base_profile::parse_base_profile;
use crate::types::class_data::{self, GEAR_SLOTS};

pub(super) fn generate_droptimizer_input(
    base_profile: &str,
    drop_items: &[Value],
) -> (String, usize, HashMap<String, Value>) {
    let (base_lines, equipped_gear, talents_string, spec) = parse_base_profile(base_profile);

    let mut lines: Vec<String> = Vec::new();
    let mut combo_metadata: HashMap<String, Value> = HashMap::new();

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

    let has_two_hand_equipped = {
        let oh = equipped_gear.get("off_hand").map(|s| s.trim());
        oh.is_none() || oh == Some("") || oh == Some(",")
    };

    // Legacy fallback: regex-copy enchant_id from equipped slot when the drop item
    // does not provide slot_inherits. Keeps direct API consumers working while
    // the frontend rolls out. Remove in a follow-up release.
    let legacy_enchant_re = Regex::new(r"enchant_id=(\d+)").unwrap();

    let mut combo_idx = 2usize;
    for item in drop_items {
        let item_id = item.get("item_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let ilevel = item.get("ilevel").and_then(|v| v.as_u64()).unwrap_or(0);
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let encounter = item
            .get("encounter")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let inv_type = item
            .get("inventory_type")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let bonus_ids: Vec<u64> = item
            .get("bonus_ids")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|b| b.as_u64()).collect())
            .unwrap_or_default();
        let slot_inherits = item.get("slot_inherits").and_then(|v| v.as_array());

        let mut slots = class_data::inv_type_to_slots(inv_type, &spec);

        if has_two_hand_equipped && !(spec == "fury" && inv_type == 17) {
            slots.retain(|s| *s != "off_hand");
        }

        if slots.is_empty() {
            continue;
        }

        let mut base_simc_str = format!(",id={},ilevel={}", item_id, ilevel);
        if !bonus_ids.is_empty() {
            let bonus_str = bonus_ids
                .iter()
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join("/");
            base_simc_str.push_str(&format!(",bonus_id={}", bonus_str));
        }

        for slot in &slots {
            let mut simc_str = base_simc_str.clone();
            let mut applied_enchant: u64 = 0;
            let mut applied_gem: u64 = 0;

            let inherit = slot_inherits.and_then(|arr| {
                arr.iter()
                    .find(|entry| entry.get("slot").and_then(|v| v.as_str()) == Some(*slot))
            });

            if let Some(entry) = inherit {
                if let Some(eid) = entry.get("enchant_id").and_then(|v| v.as_u64()) {
                    if eid > 0 {
                        simc_str.push_str(&format!(",enchant_id={}", eid));
                        applied_enchant = eid;
                    }
                }
                if let Some(gid) = entry.get("gem_id").and_then(|v| v.as_u64()) {
                    if gid > 0 {
                        simc_str.push_str(&format!(",gem_id={}", gid));
                        applied_gem = gid;
                    }
                }
            } else if let Some(equipped) = equipped_gear.get(*slot) {
                if let Some(caps) = legacy_enchant_re.captures(equipped) {
                    if let Ok(eid) = caps[1].parse::<u64>() {
                        if eid > 0 {
                            simc_str.push_str(&format!(",enchant_id={}", eid));
                            applied_enchant = eid;
                        }
                    }
                }
            }

            let combo_name = format!("Combo {}", combo_idx);
            lines.push(format!("### {}", combo_name));
            lines.push(format!(
                "profileset.\"{}\"+={}={}",
                combo_name, slot, simc_str
            ));
            if inv_type == 17 && *slot == "main_hand" && spec != "fury" {
                lines.push(format!("profileset.\"{}\"+=off_hand=,", combo_name));
            }
            if !talents_string.is_empty() {
                lines.push(format!(
                    "profileset.\"{}\"+=talents={}",
                    combo_name, talents_string
                ));
            }
            lines.push(String::new());

            combo_metadata.insert(
                combo_name.clone(),
                json!([{
                    "slot": slot,
                    "item_id": item_id,
                    "ilevel": ilevel,
                    "name": name,
                    "bonus_ids": bonus_ids,
                    "enchant_id": applied_enchant,
                    "gem_id": applied_gem,
                    "is_kept": false,
                    "encounter": encounter,
                }]),
            );
            combo_idx += 1;
        }
    }

    let combo_count = combo_idx - 2;
    (lines.join("\n"), combo_count, combo_metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drop(item_id: u64, inv_type: u64, bonus_ids: Vec<u64>) -> Value {
        json!({
            "item_id": item_id,
            "ilevel": 600,
            "name": format!("Drop {}", item_id),
            "encounter": "Boss",
            "inventory_type": inv_type,
            "bonus_ids": bonus_ids,
        })
    }

    #[test]
    fn unknown_inv_type_skipped() {
        let profile = "mage=test\nspec=frost\nhead=,id=100\n";
        let drops = vec![drop(999, 99, vec![])]; // inv_type 99 = no slots
        let (_, count, _) = generate_droptimizer_input(profile, &drops);
        assert_eq!(count, 0);
    }

    #[test]
    fn two_hand_drop_clears_off_hand_for_non_fury() {
        let profile = "\
warrior=test\n\
spec=arms\n\
main_hand=,id=200\n\
off_hand=,id=201\n";
        let drops = vec![drop(999, 17, vec![])]; // inv_type 17 = 2H weapon
        let (input, count, _) = generate_droptimizer_input(profile, &drops);
        assert_eq!(count, 1);
        assert!(input.contains("main_hand=,id=999"));
        assert!(
            input.contains("profileset.\"Combo 2\"+=off_hand=,"),
            "expected off_hand cleared for arms 2H:\n{input}"
        );
    }

    #[test]
    fn two_hand_drop_kept_dual_wield_for_fury() {
        let profile = "\
warrior=test\n\
spec=fury\n\
main_hand=,id=200\n\
off_hand=,id=201\n";
        let drops = vec![drop(999, 17, vec![])];
        let (input, _, _) = generate_droptimizer_input(profile, &drops);
        // Fury can wield two 2H weapons → off_hand should NOT be cleared
        assert!(
            !input.contains("profileset.\"Combo 2\"+=off_hand=,\n"),
            "fury should keep off_hand:\n{input}"
        );
    }

    #[test]
    fn drop_inherits_enchant_from_equipped_slot_legacy_fallback() {
        let profile = "mage=test\nspec=frost\nhead=,id=100,enchant_id=7777\n";
        let drops = vec![drop(999, 1, vec![])]; // head drop, no slot_inherits
        let (input, _, metadata) = generate_droptimizer_input(profile, &drops);
        assert!(
            input.contains(",enchant_id=7777"),
            "expected legacy enchant inheritance:\n{input}"
        );
        let combo = metadata.get("Combo 2").expect("missing combo");
        assert_eq!(combo[0]["enchant_id"], 7777);
    }

    #[test]
    fn drop_two_hand_clears_off_hand_only_when_one_hand_equipped() {
        // If user already has no off_hand (or 2H equipped without off_hand line),
        // the off_hand=, clear should still be emitted for a 2H drop on non-fury.
        let profile = "\
warrior=test\n\
spec=arms\n\
main_hand=,id=200\n";
        let drops = vec![drop(999, 17, vec![])];
        let (input, _, _) = generate_droptimizer_input(profile, &drops);
        assert!(input.contains("profileset.\"Combo 2\"+=off_hand=,"));
    }

    #[test]
    fn slot_inherits_with_zero_enchant_id_does_not_apply() {
        // slot_inherits explicitly says enchant_id=0 (no enchant on origin slot).
        let profile = "mage=test\nspec=frost\nhead=,id=100,enchant_id=7777\n";
        let drops = vec![json!({
            "item_id": 999,
            "ilevel": 600,
            "name": "Drop",
            "encounter": "Boss",
            "inventory_type": 1,
            "bonus_ids": [],
            "slot_inherits": [{ "slot": "head", "enchant_id": 0, "gem_id": 0 }]
        })];
        let (input, _, metadata) = generate_droptimizer_input(profile, &drops);
        // When slot_inherits has 0, no enchant should be added
        // (legacy fallback is bypassed since slot_inherits is present)
        let combo_line = input
            .lines()
            .find(|l| l.contains("Combo 2") && l.contains("id=999"))
            .expect("missing combo");
        assert!(
            !combo_line.contains("enchant_id="),
            "unexpected enchant: {combo_line}"
        );
        let combo = metadata.get("Combo 2").expect("missing combo");
        assert_eq!(combo[0]["enchant_id"], 0);
    }

    #[test]
    fn drop_carries_talents_when_present() {
        let profile = "mage=test\nspec=frost\nhead=,id=100\ntalents=ABCDEF\n";
        let drops = vec![drop(999, 1, vec![])];
        let (input, _, _) = generate_droptimizer_input(profile, &drops);
        assert!(input.contains("profileset.\"Combo 2\"+=talents=ABCDEF"));
    }

    #[test]
    fn drop_with_multiple_bonus_ids_joined_with_slash() {
        let profile = "mage=test\nspec=frost\nhead=,id=100\n";
        let drops = vec![drop(999, 1, vec![10, 20, 30])];
        let (input, _, _) = generate_droptimizer_input(profile, &drops);
        assert!(input.contains("bonus_id=10/20/30"));
    }

    #[test]
    fn ring_drop_emits_two_combos_one_per_finger() {
        // inv_type 11 → finger1 + finger2 → 2 emits
        let profile = "mage=test\nspec=frost\nfinger1=,id=100\nfinger2=,id=101\n";
        let drops = vec![drop(999, 11, vec![])];
        let (input, count, _) = generate_droptimizer_input(profile, &drops);
        assert_eq!(count, 2);
        assert!(input.contains("profileset.\"Combo 2\"+=finger1=,id=999"));
        assert!(input.contains("profileset.\"Combo 3\"+=finger2=,id=999"));
    }

    #[test]
    fn trinket_drop_emits_two_combos_one_per_trinket() {
        let profile = "mage=test\nspec=frost\ntrinket1=,id=100\ntrinket2=,id=101\n";
        let drops = vec![drop(999, 12, vec![])];
        let (_, count, _) = generate_droptimizer_input(profile, &drops);
        assert_eq!(count, 2);
    }

    #[test]
    fn shield_drop_targets_off_hand_only() {
        // inv_type 14 = shield → off_hand only
        let profile = "warrior=test\nspec=protection\nmain_hand=,id=100\noff_hand=,id=101\n";
        let drops = vec![drop(999, 14, vec![])];
        let (input, count, _) = generate_droptimizer_input(profile, &drops);
        assert_eq!(count, 1);
        assert!(input.contains("profileset.\"Combo 2\"+=off_hand=,id=999"));
    }

    #[test]
    fn one_hand_weapon_dual_wield_emits_two_combos() {
        // inv_type 13 = 1H, fury can dual wield
        let profile = "warrior=test\nspec=fury\nmain_hand=,id=100\noff_hand=,id=101\n";
        let drops = vec![drop(999, 13, vec![])];
        let (_, count, _) = generate_droptimizer_input(profile, &drops);
        assert_eq!(count, 2);
    }

    #[test]
    fn one_hand_weapon_non_dual_wield_emits_main_hand_only() {
        // Arms warrior cannot dual wield 1H
        let profile = "warrior=test\nspec=arms\nmain_hand=,id=100\n";
        let drops = vec![drop(999, 13, vec![])];
        let (_, count, _) = generate_droptimizer_input(profile, &drops);
        assert_eq!(count, 1);
    }

    #[test]
    fn back_drop_targets_back_slot_only() {
        let profile = "mage=test\nspec=frost\nback=,id=100\n";
        let drops = vec![drop(999, 16, vec![])];
        let (input, count, _) = generate_droptimizer_input(profile, &drops);
        assert_eq!(count, 1);
        assert!(input.contains("profileset.\"Combo 2\"+=back=,id=999"));
    }

    #[test]
    fn slot_inherits_with_mismatched_slot_does_not_apply() {
        // slot_inherits provides only finger1 entry, but inv_type 11 puts the drop
        // in both finger1 and finger2. Only finger1 should inherit.
        let profile = "\
mage=test\n\
spec=frost\n\
finger1=,id=100,enchant_id=7000,gem_id=5000\n\
finger2=,id=101\n";
        let drops = vec![json!({
            "item_id": 999,
            "ilevel": 600,
            "name": "Ring",
            "encounter": "Boss",
            "inventory_type": 11,
            "bonus_ids": [],
            "slot_inherits": [
                { "slot": "finger1", "enchant_id": 7000, "gem_id": 5000 }
            ]
        })];
        let (input, count, _) = generate_droptimizer_input(profile, &drops);
        assert_eq!(count, 2);
        assert!(input.contains("finger1=,id=999,ilevel=600,enchant_id=7000,gem_id=5000"));
        // finger2 has no slot_inherits entry → falls back to legacy regex from equipped finger2
        // finger2 has no enchant, so no enchant should appear on the finger2 combo.
        let f2_line = input
            .lines()
            .find(|l| l.contains("Combo 3") && l.contains("finger2=,id=999"))
            .expect("missing finger2 line");
        assert!(!f2_line.contains("enchant_id="), "unexpected enchant: {f2_line}");
    }

    #[test]
    fn multiple_drops_get_sequential_combo_numbers() {
        let profile = "mage=test\nspec=frost\nhead=,id=100\nchest=,id=101\n";
        let drops = vec![
            drop(901, 1, vec![]),   // head
            drop(902, 5, vec![]),   // chest
            drop(903, 16, vec![]),  // back (no equipped slot for this profile, but inv_type maps it)
        ];
        let (input, count, _) = generate_droptimizer_input(profile, &drops);
        // 3 drops, each emitting once. Even back works (it doesn't need equipped slot).
        assert_eq!(count, 3);
        assert!(input.contains("### Combo 2"));
        assert!(input.contains("### Combo 3"));
        assert!(input.contains("### Combo 4"));
    }

    #[test]
    fn drop_metadata_carries_encounter_field() {
        let profile = "mage=test\nspec=frost\nhead=,id=100\n";
        let drops = vec![json!({
            "item_id": 999,
            "ilevel": 600,
            "name": "Drop",
            "encounter": "Specific Boss Name",
            "inventory_type": 1,
            "bonus_ids": []
        })];
        let (_, _, metadata) = generate_droptimizer_input(profile, &drops);
        let combo = metadata.get("Combo 2").expect("missing combo");
        assert_eq!(combo[0]["encounter"], "Specific Boss Name");
    }
}
