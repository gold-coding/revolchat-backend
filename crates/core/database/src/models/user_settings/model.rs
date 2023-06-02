use std::collections::HashMap;

use crate::Database;

use revolt_result::Result;

pub type UserSettings = HashMap<String, (i64, String)>;

#[async_trait]
pub trait UserSettingsImpl {
    async fn set(self, db: &Database, user: &str) -> Result<()>;
}

#[async_trait]
impl UserSettingsImpl for UserSettings {
    async fn set(self, db: &Database, user: &str) -> Result<()> {
        db.set_user_settings(user, &self).await?;

        /* // TODO: EventV1::UserSettingsUpdate {
            id: user.to_string(),
            update: self,
        }
        .private(user.to_string())
        .await; */

        Ok(())
    }
}
