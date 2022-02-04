use revolt_quark::{Error, EmptyResponse, Result, models::{User, Channel}, Ref, Db, perms, ChannelPermission};

use mongodb::bson::doc;

#[put("/<target>/recipients/<member>")]
pub async fn req(
    db: &Db,
    user: User, target: Ref, member: Ref
) -> Result<EmptyResponse> {
    let channel = target.as_channel(db).await?;
    if !perms(&user).channel(&channel).calc_channel(db).await.get_invite_others() {
        return Err(Error::MissingPermission { permission: ChannelPermission::InviteOthers as i32 })
    }

    match channel {
        Channel::Group { id, .. } => {
            let user = member.as_user(db).await?;
            db.add_user_to_group(&id, &user.id).await?;
            Ok(EmptyResponse)
        }
        _ => Err(Error::InvalidOperation)
    }
}
