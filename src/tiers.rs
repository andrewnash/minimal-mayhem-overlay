use serde::Deserialize;

#[derive(Deserialize)]
pub struct Dataset {
    pub patch: String,
    augments: Vec<Augment>,
}

#[derive(Deserialize)]
struct Augment {
    name: String,
    rarity: String,
    pick_rate: f64,
}

pub struct Tiers {
    entries: Vec<(String, char)>,
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

impl Dataset {
    pub fn tiers(&self) -> Tiers {
        let entries = self
            .augments
            .iter()
            .map(|augment| {
                let peers = self
                    .augments
                    .iter()
                    .filter(|peer| peer.rarity == augment.rarity);
                let count = peers.clone().count();
                let rank = peers
                    .filter(|peer| peer.pick_rate > augment.pick_rate)
                    .count();
                let tier = ['S', 'A', 'B', 'C', 'D'][rank * 5 / count];
                (normalize(&augment.name), tier)
            })
            .collect();
        Tiers { entries }
    }
}

impl Tiers {
    pub fn recognize(&self, text: &str) -> Option<(&str, char, f64)> {
        let name = normalize(text);
        if name.is_empty() {
            return None;
        }
        let mut best = None;
        let mut runner_up: f64 = 0.0;
        for (candidate, tier) in &self.entries {
            let score = strsim::normalized_levenshtein(&name, candidate);
            if best.is_none_or(|(_, _, previous)| score > previous) {
                runner_up = best.map_or(0.0, |(_, _, score)| score);
                best = Some((candidate.as_str(), *tier, score));
            } else {
                runner_up = runner_up.max(score);
            }
        }
        best.filter(|(_, _, score)| {
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
                {"name":"Transmute: Prismatic","rarity":"gold","pick_rate":10},
                {"name":"Transmute: Gold","rarity":"silver","pick_rate":20}
            ]}"#,
        )
        .unwrap();
        let tiers = data.tiers();
        let exact = tiers.recognize("Transmute: Prismatic").unwrap();
        assert_eq!(exact.1, 'S');
        assert_eq!(tiers.recognize("Transmute Prismatlc").unwrap().0, exact.0);
        assert!(tiers.recognize("This is not a card title").is_none());
    }
}
