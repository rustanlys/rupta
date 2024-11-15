use std::rc::Rc;
use crate::PhiUser;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Path {
    pub value: PathEnum,
}

impl Path {
    pub fn undef() -> Self {
        Path {
            value: PathEnum::Undef,
        }
    }
}


#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum PathEnum {
    Value(i32), 
    Undef,
    /*  Ask about what pathes we should include?? Was thinking about just the functions, but 
        we need to consider phi nodes? More a CFG pathing algo 
    */
    
}



#[derive(Debug, Clone)]
pub struct Undef;

impl Undef {
    pub fn new() -> Self {
        Undef
    }
}
