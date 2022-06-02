use crate::events::client::EventV1;
use crate::models::user::{
    Badges, FieldsUser, PartialUser, Presence, RelationshipStatus, User, UserHint,
};
use crate::permissions::defn::UserPerms;
use crate::permissions::r#impl::user::get_relationship;
use crate::{perms, Database, Error, Result};

use futures::try_join;
use impl_ops::impl_op_ex_commutative;
use okapi::openapi3::{SecurityScheme, SecuritySchemeData};
use rocket_okapi::gen::OpenApiGenerator;
use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};
use std::ops;

impl_op_ex_commutative!(+ |a: &i32, b: &Badges| -> i32 { *a | *b as i32 });

impl User {
    /// Update user data
    pub async fn update<'a>(
        &mut self,
        db: &Database,
        partial: PartialUser,
        remove: Vec<FieldsUser>,
    ) -> Result<()> {
        for field in &remove {
            self.remove(field);
        }

        self.apply_options(partial.clone());

        db.update_user(&self.id, &partial, remove.clone()).await?;

        EventV1::UserUpdate {
            id: self.id.clone(),
            data: partial,
            clear: remove,
        }
        .p_user(self.id.clone(), db)
        .await;

        Ok(())
    }

    /// Remove a field from User object
    pub fn remove(&mut self, field: &FieldsUser) {
        match field {
            FieldsUser::Avatar => self.avatar = None,
            FieldsUser::StatusText => {
                if let Some(x) = self.status.as_mut() {
                    x.text = None;
                }
            }
            FieldsUser::StatusPresence => {
                if let Some(x) = self.status.as_mut() {
                    x.presence = None;
                }
            }
            FieldsUser::ProfileContent => {
                if let Some(x) = self.profile.as_mut() {
                    x.content = None;
                }
            }
            FieldsUser::ProfileBackground => {
                if let Some(x) = self.profile.as_mut() {
                    x.background = None;
                }
            }
        }
    }

    /// Mutate the user object to remove redundant information
    pub fn foreign(mut self) -> User {
        self.profile = None;
        self.relations = None;

        let mut badges = self.badges.unwrap_or(0);
        if let Ok(id) = ulid::Ulid::from_string(&self.id) {
            // Yes, this is hard-coded
            // No, I don't care + ratio
            if id.datetime().timestamp_millis() < 1629638578431 {
                badges = badges + Badges::EarlyAdopter;
            }
        }

        self.badges = Some(badges);

        if let Some(status) = &self.status {
            if let Some(presence) = &status.presence {
                if presence == &Presence::Invisible {
                    self.status = None;
                    self.online = Some(false);
                }
            }
        }

        self
    }

    /// Mutate the user object to include relationship (if it does not already exist)
    #[must_use]
    pub fn with_relationship(self, perspective: &User) -> User {
        let mut user = self.foreign();

        if user.relationship.is_none() {
            user.relationship = Some(get_relationship(perspective, &user.id));
        }

        user
    }

    /// Mutate user object with given permission
    #[must_use]
    pub fn apply_permission(mut self, permission: &UserPerms) -> User {
        if !permission.get_view_profile() {
            self.status = None;
        }

        self
    }

    /// Helper function to apply relationship and permission
    #[must_use]
    pub fn with_perspective(self, perspective: &User, permission: &UserPerms) -> User {
        self.with_relationship(perspective)
            .apply_permission(permission)
    }

    /// Helper function to calculate perspective
    pub async fn with_auto_perspective(self, db: &Database, perspective: &User) -> User {
        let user = self.with_relationship(perspective);
        let permissions = perms(perspective).user(&user).calc_user(db).await;
        user.apply_permission(&permissions)
    }

    /// Check whether two users have a mutual connection
    ///
    /// This will check if user and user_b share a server or a group.
    pub async fn has_mutual_connection(&self, db: &Database, user_b: &str) -> Result<bool> {
        Ok(!db
            .fetch_mutual_server_ids(&self.id, user_b)
            .await?
            .is_empty()
            || !db
                .fetch_mutual_channel_ids(&self.id, user_b)
                .await?
                .is_empty())
    }

    /// Check if this user can acquire another server
    pub async fn can_acquire_server(&self, db: &Database) -> Result<bool> {
        // ! FIXME: hardcoded max server count
        Ok(db.fetch_server_count(&self.id).await? <= 100)
    }

    /// Update a user's username
    pub async fn update_username(&mut self, db: &Database, username: String) -> Result<()> {
        let username = username.trim().to_string();

        if db.is_username_taken(&username).await? {
            return Err(Error::UsernameTaken);
        }

        self.update(
            db,
            PartialUser {
                username: Some(username),
                ..Default::default()
            },
            vec![],
        )
        .await
    }

    /// Apply a certain relationship between two users
    pub async fn apply_relationship(
        &self,
        db: &Database,
        target: &mut User,
        local: RelationshipStatus,
        remote: RelationshipStatus,
    ) -> Result<()> {
        if try_join!(
            db.set_relationship(&self.id, &target.id, &local),
            db.set_relationship(&target.id, &self.id, &remote)
        )
        .is_err()
        {
            return Err(Error::DatabaseError {
                operation: "update_one",
                with: "user",
            });
        }

        EventV1::UserRelationship {
            id: target.id.clone(),
            user: self.clone().with_relationship(target),
            status: remote,
        }
        .private(target.id.clone())
        .await;

        EventV1::UserRelationship {
            id: self.id.clone(),
            user: target.clone().with_relationship(self),
            status: local.clone(),
        }
        .private(self.id.clone())
        .await;

        target.relationship.replace(local);
        Ok(())
    }

    /// Add another user as a friend
    pub async fn add_friend(&self, db: &Database, target: &mut User) -> Result<()> {
        match get_relationship(self, &target.id) {
            RelationshipStatus::User => Err(Error::NoEffect),
            RelationshipStatus::Friend => Err(Error::AlreadyFriends),
            RelationshipStatus::Outgoing => Err(Error::AlreadySentRequest),
            RelationshipStatus::Blocked => Err(Error::Blocked),
            RelationshipStatus::BlockedOther => Err(Error::BlockedByOther),
            RelationshipStatus::Incoming => {
                self.apply_relationship(
                    db,
                    target,
                    RelationshipStatus::Friend,
                    RelationshipStatus::Friend,
                )
                .await
            }
            RelationshipStatus::None => {
                self.apply_relationship(
                    db,
                    target,
                    RelationshipStatus::Outgoing,
                    RelationshipStatus::Incoming,
                )
                .await
            }
        }
    }

    /// Remove another user as a friend
    pub async fn remove_friend(&self, db: &Database, target: &mut User) -> Result<()> {
        match get_relationship(self, &target.id) {
            RelationshipStatus::Friend
            | RelationshipStatus::Outgoing
            | RelationshipStatus::Incoming => {
                self.apply_relationship(
                    db,
                    target,
                    RelationshipStatus::None,
                    RelationshipStatus::None,
                )
                .await
            }
            _ => Err(Error::NoEffect),
        }
    }

    /// Block another user
    pub async fn block_user(&self, db: &Database, target: &mut User) -> Result<()> {
        match get_relationship(self, &target.id) {
            RelationshipStatus::User | RelationshipStatus::Blocked => Err(Error::NoEffect),
            RelationshipStatus::BlockedOther => {
                self.apply_relationship(
                    db,
                    target,
                    RelationshipStatus::Blocked,
                    RelationshipStatus::Blocked,
                )
                .await
            }
            RelationshipStatus::None
            | RelationshipStatus::Friend
            | RelationshipStatus::Incoming
            | RelationshipStatus::Outgoing => {
                self.apply_relationship(
                    db,
                    target,
                    RelationshipStatus::Blocked,
                    RelationshipStatus::BlockedOther,
                )
                .await
            }
        }
    }

    /// Unblock another user
    pub async fn unblock_user(&self, db: &Database, target: &mut User) -> Result<()> {
        match get_relationship(self, &target.id) {
            RelationshipStatus::Blocked => match get_relationship(target, &self.id) {
                RelationshipStatus::Blocked => {
                    self.apply_relationship(
                        db,
                        target,
                        RelationshipStatus::BlockedOther,
                        RelationshipStatus::Blocked,
                    )
                    .await
                }
                RelationshipStatus::BlockedOther => {
                    self.apply_relationship(
                        db,
                        target,
                        RelationshipStatus::None,
                        RelationshipStatus::None,
                    )
                    .await
                }
                _ => Err(Error::InternalError),
            },
            _ => Err(Error::NoEffect),
        }
    }

    /// Check whether this user has another user blocked
    pub fn has_blocked(&self, user: &str) -> bool {
        matches!(
            get_relationship(self, user),
            RelationshipStatus::Blocked | RelationshipStatus::BlockedOther
        )
    }

    /// Mark as deleted
    pub async fn mark_deleted(&mut self, db: &Database) -> Result<()> {
        self.update(
            db,
            PartialUser {
                username: Some(format!("Deleted User {}", self.id)),
                flags: Some(2),
                ..Default::default()
            },
            vec![
                FieldsUser::Avatar,
                FieldsUser::StatusText,
                FieldsUser::StatusPresence,
                FieldsUser::ProfileContent,
                FieldsUser::ProfileBackground,
            ],
        )
        .await
    }

    /// Find a user from a given token and hint
    #[async_recursion]
    pub async fn from_token(db: &Database, token: &str, hint: UserHint) -> Result<User> {
        match hint {
            UserHint::Bot => db.fetch_user(&db.fetch_bot_by_token(token).await?.id).await,
            UserHint::User => db.fetch_user_by_token(token).await,
            UserHint::Any => {
                if let Ok(user) = User::from_token(db, token, UserHint::User).await {
                    Ok(user)
                } else {
                    User::from_token(db, token, UserHint::Bot).await
                }
            }
        }
    }
}

use rauth::entities::Session;
use rocket::http::Status;
use rocket::request::{self, FromRequest, Outcome, Request};

#[rocket::async_trait]
impl<'r> FromRequest<'r> for User {
    type Error = rauth::util::Error;

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let user: &Option<User> = request
            .local_cache_async(async {
                let db = request
                    .rocket()
                    .state::<Database>()
                    .expect("Database state not reachable!");

                let header_bot_token = request
                    .headers()
                    .get("x-bot-token")
                    .next()
                    .map(|x| x.to_string());

                if let Some(bot_token) = header_bot_token {
                    if let Ok(user) = User::from_token(db, &bot_token, UserHint::Bot).await {
                        return Some(user);
                    }
                } else if let Outcome::Success(session) = request.guard::<Session>().await {
                    // This uses a guard so can't really easily be refactored into from_token at this stage.
                    if let Ok(user) = db.fetch_user(&session.user_id).await {
                        return Some(user);
                    }
                }

                None
            })
            .await;

        if let Some(user) = user {
            Outcome::Success(user.clone())
        } else {
            Outcome::Failure((Status::Unauthorized, rauth::util::Error::InvalidSession))
        }
    }
}

impl<'r> OpenApiFromRequest<'r> for User {
    fn from_request_input(
        _gen: &mut OpenApiGenerator,
        _name: String,
        _required: bool,
    ) -> rocket_okapi::Result<RequestHeaderInput> {
        let mut requirements = schemars::Map::new();
        requirements.insert("Api Key".to_owned(), vec![]);

        Ok(RequestHeaderInput::Security(
            "Api Key".to_owned(),
            SecurityScheme {
                data: SecuritySchemeData::ApiKey {
                    name: "x-session-token".to_owned(),
                    location: "header".to_owned(),
                },
                description: Some("Session Token".to_owned()),
                extensions: schemars::Map::new(),
            },
            requirements,
        ))
    }
}
