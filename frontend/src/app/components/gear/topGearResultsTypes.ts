import type { ReactNode } from 'react';
import type { GearItem } from './gearOverviewTypes';

export interface ResultItem extends GearItem {
  encounter?: string;
  type?: 'enchant' | 'gem';
}

export interface TopGearResult {
  name: string;
  items: ResultItem[];
  dps: number;
  talent_build?: string;
  talent_spec?: string;
  delta: number;
}

export interface TopGearResultsProps {
  playerName: string;
  playerClass: string;
  playerRealm?: string;
  playerRegion?: string;
  baseDps: number;
  results: TopGearResult[];
  equippedGear?: Record<string, ResultItem>;
  dpsError?: number;
  dpsErrorPct?: number;
  fightLength?: number;
  desiredTargets?: number;
  iterations?: number;
  targetError?: number;
  elapsedTime?: number;
  backLink?: ReactNode;
}

export type GroupMode = 'rank' | 'encounter' | 'slot';
