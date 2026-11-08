use serde::Deserialize;
use std::{ffi::c_void, ptr};
use windows::{
    Win32::Networking::WinHttp::*,
    core::{PCWSTR, w},
};

use crate::tiers::Dataset;

pub fn champion(data: &Dataset) -> Option<&str> {
    champion_from_response(data, &read_game()?)
}

fn champion_from_response<'a>(data: &'a Dataset, response: &[u8]) -> Option<&'a str> {
    let game: Game = serde_json::from_slice(response).ok()?;
    let active = &game.active_player;
    let active_id = active.riot_id.as_deref().filter(|id| !id.is_empty());
    let player = active_id
        .and_then(|id| {
            game.all_players
                .iter()
                .find(|player| player.riot_id.as_deref() == Some(id))
        })
        .or_else(|| {
            let name = active
                .summoner_name
                .as_deref()
                .filter(|name| !name.is_empty())?;
            game.all_players.iter().find(|player| {
                let player_id = player.riot_id.as_deref().filter(|id| !id.is_empty());
                (active_id.is_none() || player_id.is_none())
                    && player.summoner_name.as_deref() == Some(name)
            })
        })?;

    let raw_key = player
        .raw_champion_name
        .as_deref()
        .and_then(|name| name.strip_prefix("game_character_displayname_"))
        .map(str::to_ascii_lowercase);
    raw_key
        .as_deref()
        .and_then(|key| data.champions.get_key_value(key))
        .or_else(|| {
            let name = player.champion_name.as_deref()?;
            data.champions
                .iter()
                .find(|(_, entry)| entry.name.eq_ignore_ascii_case(name))
        })
        .map(|(key, _)| key.as_str())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Game {
    active_player: Player,
    all_players: Vec<Player>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Player {
    riot_id: Option<String>,
    summoner_name: Option<String>,
    champion_name: Option<String>,
    raw_champion_name: Option<String>,
}

struct Handle(*mut c_void);

impl Handle {
    fn new(raw: *mut c_void) -> Option<Self> {
        (!raw.is_null()).then(|| Self(raw))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let _ = unsafe { WinHttpCloseHandle(self.0) };
    }
}

fn read_game() -> Option<Vec<u8>> {
    let null = PCWSTR::null();
    unsafe {
        let session = Handle::new(WinHttpOpen(
            w!("Minimal Mayhem Overlay"),
            WINHTTP_ACCESS_TYPE_NO_PROXY,
            null,
            null,
            0,
        ))?;
        WinHttpSetTimeouts(session.0, 250, 250, 250, 250).ok()?;
        let connection = Handle::new(WinHttpConnect(session.0, w!("127.0.0.1"), 2999, 0))?;
        let request = Handle::new(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            w!("/liveclientdata/allgamedata"),
            null,
            null,
            ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        // League uses a self-signed cert. Keep the exception on this one loopback request.
        let local_cert = SECURITY_FLAG_IGNORE_UNKNOWN_CA | SECURITY_FLAG_IGNORE_CERT_CN_INVALID;
        let disabled =
            WINHTTP_DISABLE_REDIRECTS | WINHTTP_DISABLE_COOKIES | WINHTTP_DISABLE_AUTHENTICATION;
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_DISABLE_FEATURE,
            Some(&disabled.to_ne_bytes()),
        )
        .ok()?;
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_SECURITY_FLAGS,
            Some(&local_cert.to_ne_bytes()),
        )
        .ok()?;
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).ok()?;
        WinHttpReceiveResponse(request.0, ptr::null_mut()).ok()?;
        let mut status = 0u32;
        let mut size = size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null,
            Some(ptr::from_mut(&mut status).cast()),
            &mut size,
            ptr::null_mut(),
        )
        .ok()?;
        if status != 200 {
            return None;
        }
        let mut body = Vec::new();
        let mut buffer = [0; 8192];
        while body.len() < 1024 * 1024 {
            let mut read = 0;
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            )
            .ok()?;
            if read == 0 {
                return Some(body);
            }
            body.extend_from_slice(&buffer[..read as usize]);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_local_players_champion_using_riot_id() {
        let data: Dataset = serde_json::from_str(include_str!("../data/augments.json")).unwrap();
        let response = br#"{
            "activePlayer":{"riotId":"Local#NA1","summonerName":"Local"},
            "allPlayers":[
                {"summonerName":"Local","rawChampionName":"game_character_displayname_Ashe"},
                {"riotId":"Other#NA1","summonerName":"Local","rawChampionName":"game_character_displayname_Gwen"},
                {"riotId":"Local#NA1","rawChampionName":"game_character_displayname_Lucian"}
            ]
        }"#;
        assert_eq!(champion_from_response(&data, response), Some("lucian"));
        assert_eq!(champion_from_response(&data, b"{}"), None);
    }

    #[test]
    fn supports_legacy_identity_and_champion_display_name() {
        let data: Dataset = serde_json::from_str(include_str!("../data/augments.json")).unwrap();
        let response = br#"{
            "activePlayer":{"summonerName":"Local"},
            "allPlayers":[{"summonerName":"Local","championName":"Wukong"}]
        }"#;
        assert_eq!(champion_from_response(&data, response), Some("monkeyking"));
    }
}
