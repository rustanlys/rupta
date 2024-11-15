use std::rc::Rc;
use crate::PhiUser;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Path {
    pub value: PathEnum,
}
