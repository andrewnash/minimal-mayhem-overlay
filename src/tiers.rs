use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct Dataset {
    pub patch: String,
    pub stats_date: Option<String>,
    augments: Vec<Augment>,
    #[serde(default)]
    combo_hints: HashMap<String, HashMap<String, String>>,
}

#[derive(Deserialize)]
pub struct ChampionPicks {
    patch: String,
    champion: String,
    games: u32,
    picks: HashMap<String, u32>,
    #[serde(default)]
    weapon_items: Vec<String>,
}

impl ChampionPicks {
    pub fn load(patch: &str, champion: &str) -> Option<Self> {
        let path = std::env::current_exe()
            .ok()?
            .with_file_name("champion-picks.json");
        let data: Self = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        (data.patch == patch && data.champion == champion && data.games > 0).then_some(data)
    }

    pub fn match_share(&self, data: &Dataset, card: CardStats) -> Option<(f64, u32)> {
        let id = &data.augments.get(card.augment_index)?.id;
        let count = *self.picks.get(id)?;
        (count <= self.games).then_some((100.0 * f64::from(count) / f64::from(self.games), count))
    }

    pub fn weapon_items(&self, data: &Dataset, card: CardStats) -> Option<&[String]> {
        let id = &data.augments.get(card.augment_index)?.id;
        (id == "weapon_of_destiny" && !self.weapon_items.is_empty())
            .then_some(self.weapon_items.as_slice())
    }
}

#[derive(Deserialize)]
struct Augment {
    #[serde(default)]
    id: String,
    name: String,
    rarity: String,
    pick_rate: f64,
    pick_change: Option<f64>,
    rank_change: Option<i32>,
    #[serde(default)]
    new_in_data: bool,
}

pub struct Tiers {
    entries: Vec<(String, CardStats)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardStats {
    pub tier: char,
    pub pick_rate: f64,
    pub rarity_rank: usize,
    pub rarity_count: usize,
    pub pick_change: Option<f64>,
    pub rank_change: Option<i32>,
    pub new_in_data: bool,
    pub augment_index: usize,
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

impl Dataset {
    pub fn combo_hint(&self, champion: &str, card: CardStats) -> Option<&str> {
        let id = &self.augments.get(card.augment_index)?.id;
        self.combo_hints.get(champion)?.get(id).map(String::as_str)
    }

    pub fn tiers(&self) -> Tiers {
        let entries = self
            .augments
            .iter()
            .enumerate()
            .map(|(augment_index, augment)| {
                let peers = self
                    .augments
                    .iter()
                    .filter(|peer| peer.rarity == augment.rarity);
                let count = peers.clone().count();
                let rank = peers
                    .filter(|peer| peer.pick_rate > augment.pick_rate)
                    .count();
                let tier = ['S', 'A', 'B', 'C', 'D'][rank * 5 / count];
                let stats = CardStats {
                    tier,
                    pick_rate: augment.pick_rate,
                    rarity_rank: rank + 1,
                    rarity_count: count,
                    pick_change: augment.pick_change,
                    rank_change: augment.rank_change,
                    new_in_data: augment.new_in_data,
                    augment_index,
                };
                (normalize(&augment.name), stats)
            })
            .collect();
        Tiers { entries }
    }
}

impl Tiers {
    pub fn recognize(&self, text: &str) -> Option<(CardStats, f64)> {
        let name = normalize(text);
        if name.is_empty() {
            return None;
        }
        let mut best = None;
        let mut runner_up: f64 = 0.0;
        for (candidate, stats) in &self.entries {
            let score = strsim::normalized_levenshtein(&name, candidate);
            if best.is_none_or(|(_, previous)| score > previous) {
                runner_up = best.map_or(0.0, |(_, score)| score);
                best = Some((*stats, score));
            } else {
                runner_up = runner_up.max(score);
            }
        }
        best.filter(|(_, score)| {
            (*score == 1.0 && runner_up < 1.0)
                || (name.len() >= 6 && *score >= 0.83 && *score - runner_up >= 0.08)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_ocr_errors_and_unknown_titles() {
        let data: Dataset = serde_json::from_str(
            r#"{"patch":"test","augments":[
                {"id":"transmute_prismatic","name":"Transmute: Prismatic","rarity":"gold","pick_rate":10},
                {"name":"Transmute: Gold","rarity":"silver","pick_rate":20},
                {"id":"weapon_of_destiny","name":"Weapon of Destiny","rarity":"gold","pick_rate":15}
            ],"combo_hints":{"lucian":{"transmute_prismatic":"Mystic Punch"}}}"#,
        )
        .unwrap();
        let tiers = data.tiers();
        let (exact, _) = tiers.recognize("Transmute: Prismatic").unwrap();
        assert_eq!(
            exact,
            CardStats {
                tier: 'B',
                pick_rate: 10.0,
                rarity_rank: 2,
                rarity_count: 2,
                pick_change: None,
                rank_change: None,
                new_in_data: false,
                augment_index: 0,
            }
        );
        assert_eq!(tiers.recognize("Transmute Prismatlc").unwrap().0, exact);
        assert!(tiers.recognize("This is not a card title").is_none());
        assert_eq!(data.combo_hint("lucian", exact), Some("Mystic Punch"));
        assert_eq!(data.combo_hint("ashe", exact), None);
        let picks: ChampionPicks = serde_json::from_str(
            r#"{"patch":"test","champion":"lucian","games":500,"picks":{"transmute_prismatic":75},
                "weapon_items":["Serpent's Fang","The Collector"]}"#,
        )
        .unwrap();
        assert_eq!(picks.match_share(&data, exact), Some((15.0, 75)));
        assert_eq!(picks.weapon_items(&data, exact), None);
        let (weapon, _) = tiers.recognize("Weapon of Destiny").unwrap();
        assert_eq!(
            picks.weapon_items(&data, weapon).unwrap(),
            ["Serpent's Fang", "The Collector"]
        );
    }
}
