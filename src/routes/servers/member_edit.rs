use revolt_quark::{
    models::{
        server_member::{FieldsMember, PartialMember},
        File, Member, User,
    },
    perms, Db, Error, Ref, Result,
};

use rocket::serde::json::Json;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Validate, Serialize, Deserialize)]
pub struct Data {
    #[validate(length(min = 1, max = 32))]
    nickname: Option<String>,
    avatar: Option<String>,
    roles: Option<Vec<String>>,
    #[validate(length(min = 1))]
    remove: Option<Vec<FieldsMember>>,
}

#[patch("/<server>/members/<target>", data = "<data>")]
pub async fn req(
    db: &Db,
    user: User,
    server: Ref,
    target: Ref,
    data: Json<Data>,
) -> Result<Json<Member>> {
    let data = data.into_inner();
    data.validate()
        .map_err(|error| Error::FailedValidation { error })?;

    let server = server.as_server(db).await?;
    let permissions = perms(&user).server(&server).calc_server(db).await;
    if !permissions.get_view() {
        return Err(Error::NotFound);
    }

    let mut member = target.as_member(db, &server.id).await?;

    let Data {
        nickname,
        avatar,
        roles,
        remove,
    } = data;

    let mut partial = PartialMember {
        nickname,
        roles,
        ..Default::default()
    };

    // ! FIXME: calculate permission against member

    // 1. Remove fields from object
    if let Some(fields) = &remove {
        if fields.contains(&FieldsMember::Avatar) {
            if let Some(avatar) = &member.avatar {
                db.mark_attachment_as_deleted(&avatar.id).await?;
            }
        }

        for field in fields {
            member.remove(field);
        }
    }

    // 2. Apply new avatar
    if let Some(avatar) = avatar {
        partial.avatar = Some(File::use_avatar(db, &avatar, &user.id).await?);
        member.avatar = partial.avatar.clone();
    }

    // 3. Copy over other fields
    if let Some(nickname) = &partial.nickname {
        member.nickname.replace(nickname.clone());
    }

    if let Some(roles) = &partial.roles {
        member.roles.replace(roles.clone());
    }

    db.update_member(&member.id, &partial, remove.unwrap_or_else(Vec::new))
        .await?;

    Ok(Json(member))
}
