use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::util;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Accounts {
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub active: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    /// offline / microsoft / yggdrasil
    pub kind: String,
    pub name: String,
    pub uuid: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub refresh: String,
    #[serde(default)]
    pub server: String,
}

impl Accounts {
    pub fn path() -> std::path::PathBuf {
        util::base_dir().join("accounts.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        std::fs::write(Self::path(), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn active_account(&self) -> Option<&Account> {
        let id = self.active.as_ref()?;
        self.accounts.iter().find(|a| &a.id == id)
    }

    pub fn upsert(&mut self, acc: Account) {
        if let Some(pos) = self.accounts.iter().position(|a| a.id == acc.id) {
            self.accounts[pos] = acc.clone();
        } else {
            self.accounts.push(acc.clone());
        }
        self.active = Some(acc.id);
    }

    pub fn remove(&mut self, id: &str) {
        self.accounts.retain(|a| a.id != id);
        if self.active.as_deref() == Some(id) {
            self.active = self.accounts.first().map(|a| a.id.clone());
        }
    }
}

pub fn offline_uuid(name: &str) -> String {
    let digest = md5::compute(format!("OfflinePlayer:{name}").as_bytes());
    let mut u = digest.0;
    u[6] = (u[6] & 0x0f) | 0x30;
    u[8] = (u[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        u[0], u[1], u[2], u[3], u[4], u[5], u[6], u[7], u[8], u[9], u[10], u[11], u[12], u[13], u[14], u[15]
    )
}

pub fn add_offline(name: &str) -> Account {
    let name = name.trim();
    Account {
        id: format!("offline-{name}"),
        kind: "offline".into(),
        name: name.to_string(),
        uuid: offline_uuid(name),
        token: "0".into(),
        refresh: String::new(),
        server: String::new(),
    }
}

// ---------------- 微软设备码登录 ----------------
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub interval: u64,
    #[serde(default)]
    pub expires_in: u64,
}

pub fn ms_start_device_code(client_id: &str) -> Result<DeviceCode> {
    let client = crate::net::client()?;
    let resp = client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode")
        .form(&[
            ("client_id", client_id),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()?
        .error_for_status()?;
    Ok(resp.json()?)
}

/// 轮询一次；authorization_pending 时返回 Ok(None)
pub fn ms_poll_token(client_id: &str, device_code: &str) -> Result<Option<(String, String)>> {
    #[derive(Deserialize)]
    struct TokenResp {
        #[serde(default)]
        access_token: String,
        #[serde(default)]
        refresh_token: String,
        #[serde(default)]
        error: String,
    }
    let client = crate::net::client()?;
    let resp = client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&[
            ("client_id", client_id),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", device_code),
        ])
        .send()?;
    let body: TokenResp = resp.json()?;
    if body.access_token.is_empty() {
        if body.error == "authorization_pending" {
            return Ok(None);
        }
        anyhow::bail!("登录失败: {}", body.error);
    }
    Ok(Some((body.access_token, body.refresh_token)))
}

#[derive(Debug, Clone)]
pub struct McProfile {
    pub uuid: String,
    pub name: String,
}

/// 微软令牌 → Xbox → XSTS → Minecraft
pub fn ms_login_minecraft(ms_access: &str) -> Result<McProfile> {
    let client = crate::net::client()?;

    #[derive(Deserialize)]
    struct XblResp {
        #[serde(rename = "Token")]
        token: String,
        #[serde(rename = "DisplayClaims")]
        claims: XblClaims,
    }
    #[derive(Deserialize)]
    struct XblClaims {
        xui: Vec<Xui>,
    }
    #[derive(Deserialize)]
    struct Xui {
        uhs: String,
    }

    let xbl: XblResp = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&serde_json::json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()?
        .error_for_status()?
        .json()?;
    let uhs = xbl.claims.xui.first().map(|x| x.uhs.clone()).unwrap_or_default();

    let xsts: XblResp = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .json(&serde_json::json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [xbl.token]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()?
        .error_for_status()?
        .json()?;

    #[derive(Deserialize)]
    struct McToken {
        access_token: String,
    }
    let mc: McToken = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&serde_json::json!({
            "identityToken": format!("XBL3.0 x={};{}", uhs, xsts.token)
        }))
        .send()?
        .error_for_status()?
        .json()?;

    #[derive(Deserialize)]
    struct Profile {
        id: String,
        name: String,
    }
    let resp = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(&mc.access_token)
        .send()?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        anyhow::bail!("该微软账号未拥有 Minecraft（需购买 Java 版，或开通 PC Game Pass）");
    }
    let profile: Profile = resp.error_for_status()?.json()?;

    Ok(McProfile {
        uuid: profile.id,
        name: profile.name,
    })
}

// ---------------- Yggdrasil 外置登录 ----------------
pub fn yggdrasil_login(server: &str, username: &str, password: &str) -> Result<Account> {
    #[derive(Deserialize)]
    struct Resp {
        #[serde(rename = "accessToken")]
        access_token: String,
        #[serde(rename = "selectedProfile", default)]
        selected: Option<Profile>,
    }
    #[derive(Deserialize)]
    struct Profile {
        id: String,
        name: String,
    }
    let base = server.trim_end_matches('/');
    let resp: Resp = crate::net::client()?
        .post(format!("{base}/authserver/authenticate"))
        .json(&serde_json::json!({
            "agent": {"name": "Minecraft", "version": 1},
            "username": username,
            "password": password,
            "requestUser": false
        }))
        .send()
        .with_context(|| format!("无法连接 {base}"))?
        .error_for_status()?
        .json()?;
    let profile = resp.selected.context("服务器未返回角色")?;
    Ok(Account {
        id: format!("ygg-{}", profile.id),
        kind: "yggdrasil".into(),
        name: profile.name,
        uuid: profile.id,
        token: resp.access_token,
        refresh: String::new(),
        server: base.to_string(),
    })
}
