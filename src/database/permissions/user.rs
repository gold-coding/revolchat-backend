use crate::database::*;
use num_enum::TryFromPrimitive;
use std::ops;

#[derive(Debug, PartialEq, Eq, TryFromPrimitive, Copy, Clone)]
#[repr(u32)]
pub enum UserPermission {
    Access = 1,
    SendMessage = 2,
    Invite = 4,
}

bitfield! {
    pub struct UserPermissions(MSB0 [u32]);
    u32;
    pub get_access, _: 31;
    pub get_send_message, _: 30;
    pub get_invite, _: 29;
}

impl_op_ex!(+ |a: &UserPermission, b: &UserPermission| -> u32 { *a as u32 | *b as u32 });
impl_op_ex_commutative!(+ |a: &u32, b: &UserPermission| -> u32 { *a | *b as u32 });

pub async fn calculate(user: &User, target: &str) -> UserPermissions<[u32; 1]> {
    // if friends; Access + Message + Invite
    // if mutually know each other:
    //    and has DMs from users enabled -> Access + Message
    //    otherwise -> Access
    // otherwise; None

    if let RelationshipStatus::Friend = get_relationship(&user, &target) {
        UserPermissions([UserPermission::Access + UserPermission::SendMessage + UserPermission::Invite])
    } else {
        UserPermissions([ 0 ])
    }
}

pub fn get_relationship(a: &User, b: &str) -> RelationshipStatus {
    if a.id == b {
        return RelationshipStatus::Friend;
    }

    if let Some(relations) = &a.relations {
        if let Some(relationship) = relations.iter().find(|x| x.id == b) {
            return relationship.status.clone();
        }
    }

    RelationshipStatus::None
}
