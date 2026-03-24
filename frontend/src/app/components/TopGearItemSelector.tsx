"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { ItemsBySlot, ParsedItem, GEAR_SLOTS } from "../lib/parseAddonString";
import { useItemInfo, useEnchantInfo, useGemInfo, getIconUrl, getWowheadUrl, getWowheadData, QUALITY_COLORS } from "../lib/useItemInfo";
import type { ItemQuery, ItemInfo, EnchantInfo, GemInfo } from "../lib/useItemInfo";
import { useWowheadTooltips } from "../lib/useWowheadTooltips";
import { API_URL } from "../lib/api";
import { useSimContext } from "./SimContext";

interface UpgradeOption {
  bonus_id: number;
  level: number;
  max: number;
  name: string;
  fullName: string;
  itemLevel: number;
}

const ARMOR_SLOTS = new Set([
  "head", "shoulder", "chest", "wrist", "hands", "waist", "legs", "feet",
]);

interface TopGearItemSelectorProps {
  itemsBySlot: ItemsBySlot;
  selectedItems: Record<string, number[]>;
  onSelectionChange: (selected: Record<string, number[]>) => void;
  onItemsChange: (items: ItemsBySlot) => void;
  maxUpgrade?: boolean;
  maxArmorSubclass?: number | null;
}

interface DisplayGroup {
  label: string;
  slots: string[];
}

const DISPLAY_GROUPS: DisplayGroup[] = [
  { label: "Head", slots: ["head"] },
  { label: "Neck", slots: ["neck"] },
  { label: "Shoulder", slots: ["shoulder"] },
  { label: "Back", slots: ["back"] },
  { label: "Chest", slots: ["chest"] },
  { label: "Wrist", slots: ["wrist"] },
  { label: "Hands", slots: ["hands"] },
  { label: "Waist", slots: ["waist"] },
  { label: "Legs", slots: ["legs"] },
  { label: "Feet", slots: ["feet"] },
  { label: "Rings", slots: ["finger1", "finger2"] },
  { label: "Trinkets", slots: ["trinket1", "trinket2"] },
  { label: "Main Hand", slots: ["main_hand"] },
  { label: "Off Hand", slots: ["off_hand"] },
];

interface DisplayItem {
  item: ParsedItem;
  slot: string;
  index: number;
  slotLabel?: string;
}

export default function TopGearItemSelector({
  itemsBySlot,
  selectedItems,
  onSelectionChange,
  onItemsChange,
  maxUpgrade,
  maxArmorSubclass,
}: TopGearItemSelectorProps) {
  const { maxCombinations } = useSimContext();
  const [upgradeMenuFor, setUpgradeMenuFor] = useState<string | null>(null);
  const [upgradeOptions, setUpgradeOptions] = useState<UpgradeOption[]>([]);
  const [loadingUpgrades, setLoadingUpgrades] = useState(false);
  const [maxUpgradeIlevels, setMaxUpgradeIlevels] = useState<Record<string, number>>({});

  // Fetch max upgrade ilevels when toggle is on
  useEffect(() => {
    if (!maxUpgrade) {
      setMaxUpgradeIlevels({});
      return;
    }
    const items: { item_id: number; bonus_ids: number[] }[] = [];
    for (const slotItems of Object.values(itemsBySlot)) {
      for (const item of slotItems) {
        if (item.item_id > 0 && item.bonus_ids.length > 0) {
          items.push({ item_id: item.item_id, bonus_ids: item.bonus_ids });
        }
      }
    }
    if (items.length === 0) return;
    fetch(`${API_URL}/api/max-upgrade-ilevels`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(items),
    })
      .then((r) => r.json())
      .then((data) => setMaxUpgradeIlevels(data))
      .catch(() => setMaxUpgradeIlevels({}));
  }, [maxUpgrade, itemsBySlot]);

  const openUpgradeMenu = useCallback(async (item: ParsedItem, slot: string, key: string) => {
    if (upgradeMenuFor === key) {
      setUpgradeMenuFor(null);
      return;
    }
    setUpgradeMenuFor(key);
    setLoadingUpgrades(true);
    try {
      const res = await fetch(`${API_URL}/api/upgrade-options?bonus_ids=${item.bonus_ids.join(",")}`);
      const data = await res.json();
      setUpgradeOptions(data.options || []);
    } catch {
      setUpgradeOptions([]);
    }
    setLoadingUpgrades(false);
  }, [upgradeMenuFor]);

  const addUpgradedCopy = useCallback((item: ParsedItem, slot: string, option: UpgradeOption) => {
    // Find the current upgrade bonus_id to replace
    const currentUpgradeBonusId = upgradeOptions.find(
      o => item.bonus_ids.includes(o.bonus_id)
    )?.bonus_id;
    if (!currentUpgradeBonusId) return;

    // Build new bonus_ids
    const newBonusIds = item.bonus_ids.map(b => b === currentUpgradeBonusId ? option.bonus_id : b);

    // Replace bonus_id value in simc_string using regex (order/separator agnostic)
    const newSimcString = item.simc_string.replace(
      /bonus_id=[0-9/:]+/,
      `bonus_id=${newBonusIds.join("/")}`
    );

    const copy: ParsedItem = {
      ...item,
      bonus_ids: newBonusIds,
      simc_string: newSimcString,
      ilevel: option.itemLevel,
      is_equipped: false,
    };

    // Add copy to itemsBySlot
    const updated = { ...itemsBySlot };
    updated[slot] = [...(updated[slot] || []), copy];
    onItemsChange(updated);

    // Auto-select the new copy
    const newIdx = updated[slot].length - 1;
    const sel = { ...selectedItems };
    sel[slot] = [...(sel[slot] || []), newIdx];
    onSelectionChange(sel);

    setUpgradeMenuFor(null);
  }, [itemsBySlot, selectedItems, upgradeOptions, onItemsChange, onSelectionChange]);
  const allItemQueries = useMemo(() => {
    const seen = new Set<string>();
    const queries: ItemQuery[] = [];
    for (const items of Object.values(itemsBySlot)) {
      for (const item of items) {
        if (item.item_id <= 0) continue;
        const key = `${item.item_id}:${[...(item.bonus_ids || [])].sort().join(":")}`;
        if (!seen.has(key)) {
          seen.add(key);
          queries.push({ item_id: item.item_id, bonus_ids: item.bonus_ids });
        }
      }
    }
    return queries;
  }, [itemsBySlot]);

  const itemInfoMap = useItemInfo(allItemQueries);

  const allEnchantIds = useMemo(() => {
    const ids = new Set<number>();
    for (const items of Object.values(itemsBySlot)) {
      for (const item of items) {
        if (item.enchant_id > 0) ids.add(item.enchant_id);
      }
    }
    return [...ids];
  }, [itemsBySlot]);

  const enchantInfoMap = useEnchantInfo(allEnchantIds);

  const allGemIds = useMemo(() => {
    const ids = new Set<number>();
    for (const items of Object.values(itemsBySlot)) {
      for (const item of items) {
        if (item.gem_id > 0) ids.add(item.gem_id);
      }
    }
    return [...ids];
  }, [itemsBySlot]);

  const gemInfoMap = useGemInfo(allGemIds);
  useWowheadTooltips([itemInfoMap]);

  const getIlevel = (di: DisplayItem) => {
    const info = di.item.item_id > 0 ? itemInfoMap[di.item.item_id] : null;
    return di.item.ilevel || info?.ilevel || 0;
  };

  const getMaxUpgradeIlevel = (item: ParsedItem): number | undefined => {
    if (!maxUpgrade || !item.bonus_ids.length) return undefined;
    const key = `${item.item_id}:${[...item.bonus_ids].sort((a, b) => a - b).join(",")}`;
    const maxIlvl = maxUpgradeIlevels[key];
    const currentIlvl = item.ilevel || 0;
    return maxIlvl && maxIlvl > currentIlvl ? maxIlvl : undefined;
  };

  // Detect dual-wield: both main_hand and off_hand have equipped weapons
  const isDualWield = useMemo(() => {
    const mh = itemsBySlot.main_hand?.find((i) => i.is_equipped);
    const oh = itemsBySlot.off_hand?.find((i) => i.is_equipped);
    return !!mh && !!oh;
  }, [itemsBySlot]);

  const visibleGroups = useMemo(() => {
    const result: {
      group: DisplayGroup;
      equipped: DisplayItem[];
      alternatives: DisplayItem[];
    }[] = [];
    for (const group of DISPLAY_GROUPS) {
      const equipped: DisplayItem[] = [];
      const alternatives: DisplayItem[] = [];
      const seenAltKeys = new Set<string>();

      // For dual-wield, determine the "other" weapon slot to pull crossover items from
      const otherWeaponSlot =
        isDualWield && group.slots.length === 1
          ? group.slots[0] === "main_hand"
            ? "off_hand"
            : group.slots[0] === "off_hand"
            ? "main_hand"
            : null
          : null;

      // Collect items from this group's slots + crossover weapon items
      const slotsToScan = [...group.slots];
      if (otherWeaponSlot) slotsToScan.push(otherWeaponSlot);

      for (let si = 0; si < slotsToScan.length; si++) {
        const scanSlot = slotsToScan[si];
        const isOtherWeaponSlot = si >= group.slots.length;
        const displaySlot = isOtherWeaponSlot ? group.slots[0] : scanSlot;
        const items = itemsBySlot[scanSlot];
        if (!items) continue;
        for (let idx = 0; idx < items.length; idx++) {
          const item = items[idx];

          // For crossover items: skip equipped and non-1H weapons
          if (isOtherWeaponSlot) {
            if (item.is_equipped) continue;
            const info = item.item_id > 0 ? itemInfoMap[item.item_id] : null;
            // Only include confirmed one-hand weapons (inv_type 13). Skip if unknown.
            if (info?.inventory_type !== 13) continue;
          }

          if (item.is_equipped && !isOtherWeaponSlot) {
            const eqKey = `${item.item_id}:${[...item.bonus_ids].sort((a, b) => a - b).join(",")}`;
            seenAltKeys.add(eqKey); // prevent duplicates from crossover
            equipped.push({
              item,
              slot: displaySlot,
              index: idx,
              slotLabel:
                group.slots.length > 1 ? `Slot ${si + 1}` : undefined,
            });
          } else {
            const key = `${item.item_id}:${[...item.bonus_ids].sort((a, b) => a - b).join(",")}`;
            if (seenAltKeys.has(key)) continue;
            // Filter by armor type compatibility
            if (maxArmorSubclass != null && ARMOR_SLOTS.has(scanSlot)) {
              const info = item.item_id > 0 ? itemInfoMap[item.item_id] : null;
              const sub = info?.armor_subclass;
              if (sub != null && sub > 0 && sub > maxArmorSubclass) continue;
            }
            seenAltKeys.add(key);
            alternatives.push({ item, slot: displaySlot, index: idx });
          }
        }
      }
      if (equipped.length > 0 || alternatives.length > 0) {
        equipped.sort((a, b) => getIlevel(b) - getIlevel(a));
        alternatives.sort((a, b) => getIlevel(b) - getIlevel(a));
        result.push({ group, equipped, alternatives });
      }
    }
    return result;
  }, [itemsBySlot, itemInfoMap, maxArmorSubclass, isDualWield]); // eslint-disable-line react-hooks/exhaustive-deps

  function toggleItem(displayItem: DisplayItem, group: DisplayGroup) {
    const isSelecting = !isItemSelected(displayItem, group);

    // Vault constraint: only one vault item can be selected at a time
    if (isSelecting && displayItem.item.origin === "vault" && selectedVaultCount > 0) {
      // Deselect the currently selected vault item first
      const cleared = { ...selectedItems };
      for (const slot of GEAR_SLOTS) {
        const items = itemsBySlot[slot];
        if (!items) continue;
        const current = cleared[slot] || [];
        cleared[slot] = current.filter(
          (idx) => idx >= items.length || items[idx].origin !== "vault"
        );
      }
      // Now proceed with selecting the new vault item using the cleared state
      applyToggle(displayItem, group, cleared);
      return;
    }

    applyToggle(displayItem, group, { ...selectedItems });
  }

  function applyToggle(displayItem: DisplayItem, group: DisplayGroup, updated: Record<string, number[]>) {
    if (group.slots.length === 1) {
      const slot = displayItem.slot;
      const current = updated[slot] || [];
      updated[slot] = current.includes(displayItem.index)
        ? current.filter((i) => i !== displayItem.index)
        : [...current, displayItem.index];
    } else {
      const itemId = displayItem.item.item_id;
      const isCurrentlySelected = group.slots.some((slot) => {
        const items = itemsBySlot[slot];
        if (!items) return false;
        const idx = items.findIndex(
          (it) => !it.is_equipped && it.item_id === itemId
        );
        return idx >= 0 && (updated[slot] || []).includes(idx);
      });
      for (const slot of group.slots) {
        const items = itemsBySlot[slot];
        if (!items) continue;
        const idx = items.findIndex(
          (it) => !it.is_equipped && it.item_id === itemId
        );
        if (idx < 0) continue;
        const current = updated[slot] || [];
        if (isCurrentlySelected) {
          updated[slot] = current.filter((i) => i !== idx);
        } else if (!current.includes(idx)) {
          updated[slot] = [...current, idx];
        }
      }
    }
    onSelectionChange(updated);
  }

  function isItemSelected(
    displayItem: DisplayItem,
    group: DisplayGroup
  ): boolean {
    if (group.slots.length === 1) {
      return (selectedItems[displayItem.slot] || []).includes(
        displayItem.index
      );
    }
    return group.slots.some((slot) => {
      const items = itemsBySlot[slot];
      if (!items) return false;
      const idx = items.findIndex(
        (it) => !it.is_equipped && it.item_id === displayItem.item.item_id
      );
      return idx >= 0 && (selectedItems[slot] || []).includes(idx);
    });
  }

  // Count unique selected vault items (deduplicate across paired slots by item_id)
  const selectedVaultCount = useMemo(() => {
    const seenVaultIds = new Set<number>();
    for (const slot of GEAR_SLOTS) {
      const items = itemsBySlot[slot];
      if (!items) continue;
      const selected = selectedItems[slot] || [];
      for (const idx of selected) {
        if (idx < items.length && items[idx].origin === "vault") {
          seenVaultIds.add(items[idx].item_id);
        }
      }
    }
    return seenVaultIds.size;
  }, [itemsBySlot, selectedItems]);

  const comboCount = calculateCombinations(itemsBySlot, selectedItems);

  if (visibleGroups.length === 0) {
    return (
      <div className="card p-8 text-center">
        <p className="text-sm text-muted">
          No alternative items found. Make sure your SimC addon exports bag
          items.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <p className="text-xs font-medium text-muted uppercase tracking-widest">
          Select Items
        </p>
        <span
          className={`text-xs font-mono px-2.5 py-1 rounded-md ${
            comboCount > maxCombinations
              ? "bg-red-500/10 text-red-400"
              : comboCount > 0
              ? "bg-surface-2 text-white"
              : "bg-surface-2 text-muted"
          }`}
        >
          {comboCount.toLocaleString()} combo{comboCount !== 1 ? "s" : ""}
        </span>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
        {visibleGroups.map(({ group, equipped, alternatives }) => (
          <div key={group.label} className="card p-3.5 space-y-1">
            <p className="text-[10px] font-semibold text-muted uppercase tracking-widest mb-2">
              {group.label}
            </p>

            {equipped.map((di, eqIdx) => {
              const info =
                di.item.item_id > 0 ? itemInfoMap[di.item.item_id] : null;
              const qc = info
                ? QUALITY_COLORS[info.quality] || "#fff"
                : "#fff";
              const name =
                info?.name || di.item.name || `Item ${di.item.item_id}`;
              const icon = info?.icon || "inv_misc_questionmark";

              return (
                <div
                  key={`eq-${eqIdx}`}
                  className="flex items-center gap-2 px-2 py-1.5 rounded-md bg-white/[0.03]"
                >
                  <div className="w-4 h-4 rounded-[3px] bg-white/10 flex items-center justify-center shrink-0">
                    <svg
                      className="w-2.5 h-2.5 text-white/40"
                      viewBox="0 0 16 16"
                      fill="none"
                    >
                      <path
                        d="M12 5L6.5 10.5L4 8"
                        stroke="currentColor"
                        strokeWidth="2"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      />
                    </svg>
                  </div>
                  <div className="w-6 h-6 shrink-0 rounded overflow-hidden ring-1 ring-white/5">
                    {/* eslint-disable-next-line @next/next/no-img-element */}
                    <img
                      src={getIconUrl(icon)}
                      alt=""
                      width={24}
                      height={24}
                      className="w-full h-full"
                      loading="lazy"
                    />
                  </div>
                  <ItemDetails
                    di={di}
                    info={info}
                    enchant={di.item.enchant_id > 0 ? enchantInfoMap[di.item.enchant_id] : undefined}
                    gem={di.item.gem_id > 0 ? gemInfoMap[di.item.gem_id] : undefined}
                    qc={qc}
                    name={name}
                    maxUpgradeIlevel={getMaxUpgradeIlevel(di.item)}
                    upgradeMenuKey={`${di.slot}-${di.index}`}
                    upgradeMenuFor={upgradeMenuFor}
                    upgradeOptions={upgradeOptions}
                    loadingUpgrades={loadingUpgrades}
                    onUpgradeClick={() => openUpgradeMenu(di.item, di.slot, `${di.slot}-${di.index}`)}
                    onUpgradeSelect={(opt) => addUpgradedCopy(di.item, di.slot, opt)}
                  />
                </div>
              );
            })}

            {equipped.length > 0 && alternatives.length > 0 && (
              <div className="border-t border-border/50 !my-1.5" />
            )}

            {alternatives.map((di, altIdx) => {
              const checked = isItemSelected(di, group);
              const info =
                di.item.item_id > 0 ? itemInfoMap[di.item.item_id] : null;
              const qc = info
                ? QUALITY_COLORS[info.quality] || "#fff"
                : "#fff";
              const name =
                info?.name || di.item.name || `Item ${di.item.item_id}`;
              const icon = info?.icon || "inv_misc_questionmark";

              const isVault = di.item.origin === "vault";

              return (
                <label
                  key={`alt-${altIdx}`}
                  className={`flex items-center gap-2 px-2 py-1.5 rounded-md cursor-pointer transition-colors group ${
                    checked
                      ? isVault ? "bg-amber-400/[0.12] ring-2 ring-amber-400/50" : "bg-gold/[0.07]"
                      : isVault ? "bg-amber-400/[0.04] ring-1 ring-amber-400/30 hover:ring-amber-400/50 hover:bg-amber-400/[0.08]" : "hover:bg-white/[0.02]"
                  }`}
                >
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={() => toggleItem(di, group)}
                    className="sr-only peer"
                  />
                  <div
                    className={`w-4 h-4 rounded-[3px] border transition-all shrink-0 flex items-center justify-center ${
                      checked
                        ? "bg-gold border-gold"
                        : "border-gray-600 group-hover:border-gray-500"
                    }`}
                  >
                    {checked && (
                      <svg
                        className="w-2.5 h-2.5 text-black"
                        viewBox="0 0 16 16"
                        fill="none"
                      >
                        <path
                          d="M12 5L6.5 10.5L4 8"
                          stroke="currentColor"
                          strokeWidth="2"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        />
                      </svg>
                    )}
                  </div>
                  <div className={`w-6 h-6 shrink-0 rounded overflow-hidden ring-2 ${isVault ? "ring-amber-400/70" : "ring-white/5"}`}>
                    {/* eslint-disable-next-line @next/next/no-img-element */}
                    <img
                      src={getIconUrl(icon)}
                      alt=""
                      width={24}
                      height={24}
                      className="w-full h-full"
                      loading="lazy"
                    />
                  </div>
                  <ItemDetails
                    di={di}
                    info={info}
                    enchant={di.item.enchant_id > 0 ? enchantInfoMap[di.item.enchant_id] : undefined}
                    gem={di.item.gem_id > 0 ? gemInfoMap[di.item.gem_id] : undefined}
                    qc={qc}
                    name={name}
                    maxUpgradeIlevel={getMaxUpgradeIlevel(di.item)}
                    upgradeMenuKey={`${di.slot}-${di.index}`}
                    upgradeMenuFor={upgradeMenuFor}
                    upgradeOptions={upgradeOptions}
                    loadingUpgrades={loadingUpgrades}
                    onUpgradeClick={() => openUpgradeMenu(di.item, di.slot, `${di.slot}-${di.index}`)}
                    onUpgradeSelect={(opt) => addUpgradedCopy(di.item, di.slot, opt)}
                  />
                </label>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

function ItemDetails({
  di,
  info,
  enchant,
  gem,
  qc,
  name,
  maxUpgradeIlevel,
  upgradeMenuKey,
  upgradeMenuFor,
  upgradeOptions,
  loadingUpgrades,
  onUpgradeClick,
  onUpgradeSelect,
}: {
  di: DisplayItem;
  info: ItemInfo | null;
  enchant?: EnchantInfo;
  gem?: GemInfo;
  qc: string;
  name: string;
  maxUpgradeIlevel?: number;
  upgradeMenuKey: string;
  upgradeMenuFor: string | null;
  upgradeOptions: UpgradeOption[];
  loadingUpgrades: boolean;
  onUpgradeClick: () => void;
  onUpgradeSelect: (opt: UpgradeOption) => void;
}) {
  const ilevel = di.item.ilevel || info?.ilevel || 0;
  const tag = info?.tag;
  const upgrade = info?.upgrade;
  const sockets = info?.sockets;
  const hasUpgrade = !!upgrade;
  const isMenuOpen = upgradeMenuFor === upgradeMenuKey;

  const isVault = di.item.origin === "vault";

  // Build subtitle parts
  const parts: { text: string; color?: string }[] = [];
  if (isVault) parts.push({ text: "Great Vault", color: "text-amber-400/80" });
  if (tag) parts.push({ text: tag });
  if (upgrade) parts.push({ text: upgrade });
  if (gem?.name) {
    parts.push({ text: gem.name, color: "text-sky-400/70" });
  } else if (sockets && sockets > 0) {
    parts.push({ text: `${sockets > 1 ? sockets + " " : ""}Socket${sockets > 1 ? "s" : ""}`, color: "text-sky-400/70" });
  }
  if (enchant?.name) parts.push({ text: enchant.name, color: "text-emerald-400/70" });

  return (
    <>
      <div className="flex-1 min-w-0 relative">
        <a
          href={di.item.item_id > 0 ? getWowheadUrl(di.item.item_id) : undefined}
          data-wowhead={di.item.item_id > 0 ? getWowheadData(di.item.bonus_ids, di.item.ilevel, di.item.enchant_id, di.item.gem_id) : undefined}
          className="text-[12px] truncate block no-underline"
          style={{ color: qc }}
          target="_blank"
          rel="noopener noreferrer"
          onClick={(e) => e.preventDefault()}
        >
          {name}
        </a>
        {parts.length > 0 && (
          <span className="text-[9px] text-muted truncate block">
            {parts.map((p, i) => (
              <span key={i}>
                {i > 0 && <span className="opacity-40"> · </span>}
                <span className={p.color || ""}>{p.text}</span>
              </span>
            ))}
          </span>
        )}
        {isMenuOpen && (
          <div className="absolute left-0 top-full mt-1 z-50 bg-surface border border-border rounded-lg shadow-xl py-1 min-w-[180px]">
            {loadingUpgrades ? (
              <div className="px-3 py-2 text-[11px] text-muted">Loading...</div>
            ) : upgradeOptions.length === 0 ? (
              <div className="px-3 py-2 text-[11px] text-muted">No options</div>
            ) : (
              upgradeOptions.map((opt) => {
                const isCurrent = di.item.bonus_ids.includes(opt.bonus_id);
                return (
                  <button
                    key={opt.bonus_id}
                    type="button"
                    disabled={isCurrent}
                    onClick={(e) => { e.stopPropagation(); onUpgradeSelect(opt); }}
                    className={`w-full text-left px-3 py-1.5 text-[11px] flex items-center justify-between gap-2 ${
                      isCurrent
                        ? "text-muted cursor-default"
                        : "text-gray-300 hover:bg-white/[0.05] hover:text-white"
                    }`}
                  >
                    <span>{opt.fullName}</span>
                    <span className="font-mono tabular-nums text-[10px] text-muted">{opt.itemLevel}</span>
                  </button>
                );
              })
            )}
          </div>
        )}
      </div>
      <div className="flex items-center gap-1 shrink-0">
        {hasUpgrade && (
          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); e.preventDefault(); onUpgradeClick(); }}
            className={`w-5 h-5 rounded flex items-center justify-center transition-colors ${
              isMenuOpen ? "bg-gold/20 text-gold" : "text-gray-600 hover:text-gray-400 hover:bg-white/[0.05]"
            }`}
            title="Add copy at different upgrade level"
          >
            <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <path d="M8 12V4M5 7l3-3 3 3" />
            </svg>
          </button>
        )}
        <span className="text-[10px] font-mono tabular-nums">
          {maxUpgradeIlevel ? (
            <>
              <span className="text-muted line-through opacity-50">{ilevel}</span>
              <span className="text-emerald-400 ml-1">{maxUpgradeIlevel}</span>
            </>
          ) : (
            <span className="text-muted">{ilevel > 0 && ilevel}</span>
          )}
        </span>
      </div>
    </>
  );
}

function calculateCombinations(
  itemsBySlot: ItemsBySlot,
  selectedItems: Record<string, number[]>
): number {
  let total = 1;
  let hasAlternative = false;
  for (const slot of GEAR_SLOTS) {
    const items = itemsBySlot[slot];
    if (!items) continue;
    const selected = selectedItems[slot] || [];
    const altCount = selected.filter(
      (idx) => idx < items.length && !items[idx].is_equipped
    ).length;
    if (altCount > 0) {
      total *= altCount + 1;
      hasAlternative = true;
    }
  }
  return hasAlternative ? total - 1 : 0;
}
