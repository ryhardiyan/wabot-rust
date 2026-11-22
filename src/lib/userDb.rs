use crate::{config::clean_number, database::JsonDatabase};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UserData {
    pub users: HashMap<String, Value>,
    #[serde(rename = "jidMap")]
    pub jid_map: HashMap<String, String>,
    #[serde(rename = "lidMap")]
    pub lid_map: HashMap<String, String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}
pub type UserDatabase = JsonDatabase<UserData>;

impl UserData {
    pub fn resolve(&self, sender: &str) -> String {
        self.jid_map
            .get(sender)
            .or_else(|| self.lid_map.get(sender))
            .cloned()
            .unwrap_or_else(|| {
                if sender.ends_with("@lid") {
                    format!("lid:{}", sender.split('@').next().unwrap_or(sender))
                } else {
                    clean_number(sender)
                }
            })
    }
    /// LID/PN hanya ditautkan dari sender_alt yang diberikan protokol, bukan input command pengguna.
    pub fn touch(
        &mut self,
        sender: &str,
        alternate: Option<&str>,
        name: &str,
        owner: bool,
    ) -> Result<Value> {
        let phone_jid = std::iter::once(sender)
            .chain(alternate)
            .find(|s| s.ends_with("@s.whatsapp.net"));
        let id = phone_jid
            .map(clean_number)
            .unwrap_or_else(|| self.resolve(sender));
        if id.is_empty() {
            bail!("Identitas pengirim kosong");
        }
        let old_id = self.resolve(sender);
        if old_id != id {
            if let Some(old) = self.users.remove(&old_id) {
                self.users.entry(id.clone()).or_insert(old);
            }
            for mapped in self.jid_map.values_mut().chain(self.lid_map.values_mut()) {
                if *mapped == old_id {
                    *mapped = id.clone();
                }
            }
        }
        for address in std::iter::once(sender).chain(alternate) {
            let map = if address.ends_with("@lid") {
                &mut self.lid_map
            } else {
                &mut self.jid_map
            };
            map.insert(address.into(), id.clone());
        }
        let now = chrono::Utc::now().with_timezone(
            &chrono::FixedOffset::east_opt(7 * 3600)
                .ok_or_else(|| anyhow::anyhow!("Zona waktu tidak valid"))?,
        );
        let user = self.users.entry(id.clone()).or_insert_with(|| json!({"id":id,"phone":phone_jid.map(clean_number).unwrap_or_default(),"name":name,"registered":false,"join":{"date":now.format("%d/%m/%Y").to_string(),"time":now.format("%H:%M:%S").to_string()},"registeredAt":null}));
        anyhow::ensure!(user.is_object(), "Record user JSON harus berupa object");
        user["id"] = json!(id);
        if !name.is_empty() {
            user["name"] = json!(name);
        }
        for address in std::iter::once(sender).chain(alternate) {
            user[if address.ends_with("@lid") {
                "lid"
            } else {
                "jid"
            }] = json!(address);
        }
        if let Some(pn) = phone_jid {
            user["phone"] = json!(clean_number(pn));
        }
        if owner {
            user["registered"] = json!(true);
        }
        Ok(user.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lids_are_not_treated_as_phone_numbers() -> Result<()> {
        let mut d = UserData::default();
        let a = d.touch("987654321@lid", None, "A", false)?;
        assert_eq!(a["phone"], "");
        d.touch(
            "987654321@lid",
            Some("628123456789@s.whatsapp.net"),
            "A",
            false,
        )?;
        assert_eq!(d.users.len(), 1);
        assert_eq!(d.resolve("987654321@lid"), "628123456789");
        Ok(())
    }
}
