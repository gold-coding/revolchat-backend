use revolt_quark::{
    models::{Channel, User},
    Db, EmptyResponse, Error, Ref, Result,
};

#[delete("/<target>/recipients/<member>")]
pub async fn req(db: &Db, user: User, target: Ref, member: Ref) -> Result<EmptyResponse> {
    let channel = target.as_channel(db).await?;

    match &channel {
        Channel::Group {
            owner, recipients, ..
        } => {
            if &user.id != owner {
                return Err(Error::MissingPermission { permission: 0 });
            }

            let member = member.as_user(db).await?;
            if user.id == member.id {
                return Err(Error::CannotRemoveYourself);
            }

            if !recipients.iter().any(|x| *x == member.id) {
                return Err(Error::NotInGroup);
            }

            channel
                .remove_user_from_group(db, &member.id)
                .await
                .map(|_| EmptyResponse)
        }
        _ => Err(Error::InvalidOperation),
    }
}
