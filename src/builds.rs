pub struct Build {
    pub name: &'static str,
    pub core: [&'static str; 3],
    pub options: [&'static str; 2],
}

pub fn for_champion(champion: &str, role: &str) -> Option<Build> {
    let (name, core, options) = match (champion, role) {
        ("lucian" | "sivir" | "xayah", _) => (
            "Crit caster",
            ["Essence Reaver", "Infinity Edge", "Lord Dominik's Regards"],
            ["Mortal Reminder", "Rapid Firecannon"],
        ),
        ("smolder", _) => (
            "Scaling caster",
            ["Essence Reaver", "Spear of Shojin", "Rapid Firecannon"],
            ["Lord Dominik's Regards", "Mortal Reminder"],
        ),
        ("ezreal", _) => (
            "Spellblade poke",
            ["Trinity Force", "Manamune", "Spear of Shojin"],
            ["Serylda's Grudge", "Frozen Heart"],
        ),
        ("vayne" | "kogmaw" | "kalista" | "varus" | "kayle" | "belveth" | "masteryi", _) => (
            "On-hit",
            [
                "Blade of the Ruined King",
                "Guinsoo's Rageblade",
                "Terminus",
            ],
            ["Wit's End", "Kraken Slayer"],
        ),
        ("samira" | "nilah" | "jhin" | "missfortune" | "aphelios", _) => (
            "Burst crit",
            ["The Collector", "Infinity Edge", "Lord Dominik's Regards"],
            ["Mortal Reminder", "Bloodthirster"],
        ),
        ("brand" | "zyra" | "malzahar" | "swain" | "karthus" | "cassiopeia", _) => (
            "Sustained AP",
            ["Blackfire Torch", "Liandry's Torment", "Rabadon's Deathcap"],
            ["Void Staff", "Zhonya's Hourglass"],
        ),
        ("diana" | "gwen" | "mordekaiser" | "rumble" | "sylas" | "udyr", _) => (
            "AP skirmisher",
            ["Riftmaker", "Liandry's Torment", "Zhonya's Hourglass"],
            ["Nashor's Tooth", "Cosmic Drive"],
        ),
        ("akali" | "evelynn" | "fizz" | "katarina" | "leblanc" | "ekko", _) => (
            "AP burst",
            ["Lich Bane", "Shadowflame", "Rabadon's Deathcap"],
            ["Void Staff", "Zhonya's Hourglass"],
        ),
        ("kassadin" | "ryze", _) => (
            "Mana scaling",
            ["Rod of Ages", "Archangel's Staff", "Rabadon's Deathcap"],
            ["Void Staff", "Frozen Heart"],
        ),
        (
            "janna" | "lulu" | "milio" | "nami" | "renata" | "sona" | "soraka" | "yuumi" | "ivern",
            _,
        ) => (
            "Enchanter",
            ["Moonstone Renewer", "Redemption", "Dawncore"],
            ["Ardent Censer", "Mikael's Blessing"],
        ),
        (
            "braum" | "leona" | "nautilus" | "rell" | "thresh" | "alistar" | "rakan" | "taric"
            | "blitzcrank",
            _,
        ) => (
            "Team tank",
            ["Bandlepipes", "Locket of the Iron Solari", "Knight's Vow"],
            ["Frozen Heart", "Redemption"],
        ),
        (_, "Marksman") => (
            "Crit marksman",
            [
                "Yun Tal Wildarrows",
                "Infinity Edge",
                "Lord Dominik's Regards",
            ],
            ["Mortal Reminder", "Rapid Firecannon"],
        ),
        (_, "Mage") => (
            "AP caster",
            ["Luden's Echo", "Shadowflame", "Rabadon's Deathcap"],
            ["Void Staff", "Zhonya's Hourglass"],
        ),
        (_, "Fighter") => (
            "AD bruiser",
            ["Sundered Sky", "Black Cleaver", "Sterak's Gage"],
            ["Death's Dance", "Spirit Visage"],
        ),
        (_, "Tank") => (
            "Frontline tank",
            ["Heartsteel", "Unending Despair", "Jak'Sho, The Protean"],
            ["Thornmail", "Kaenic Rookern"],
        ),
        (_, "Assassin") => (
            "AD burst",
            ["Youmuu's Ghostblade", "Opportunity", "Serylda's Grudge"],
            ["Edge of Night", "Serpent's Fang"],
        ),
        (_, "Support") => (
            "Team utility",
            ["Locket of the Iron Solari", "Redemption", "Knight's Vow"],
            ["Frozen Heart", "Mikael's Blessing"],
        ),
        _ => return None,
    };
    Some(Build {
        name,
        core,
        options,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiers::Dataset;

    #[test]
    fn bundled_champions_have_routes_and_overrides_take_priority() {
        let data: Dataset = serde_json::from_str(include_str!("../data/augments.json")).unwrap();
        assert!(!data.champions.is_empty());
        for (key, champion) in &data.champions {
            assert!(for_champion(key, &champion.role).is_some(), "{key}");
        }
        assert_eq!(
            for_champion("lucian", "Marksman").unwrap().core[0],
            "Essence Reaver"
        );
        assert_eq!(
            for_champion("gwen", "Fighter").unwrap().core[0],
            "Riftmaker"
        );
    }
}
